use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::{DateTime, Local};
use serde_json::Value;
use tokio::sync::mpsc;
use tracing::debug;

use crate::detect::registry::AgentRegistry;
use crate::event::AgentEvent;
use crate::types::{
    from_epoch_secs, AgentId, AgentTool, NormalizedReason, NormalizedState, NormalizedStatus,
    PrimaryStatus,
};

const CONSERVATIVE_PID_BINDING_WINDOW_SECS: i64 = 300;

// ── Types ─────────────────────────────────────────────────────────

/// A parsed event from a Codex rollout JSONL file.
#[derive(Debug, Clone)]
pub struct RolloutEvent {
    pub timestamp: String,
    pub event_type: String,
    pub payload: Value,
}

/// Session metadata extracted from a rollout's `session_meta` event.
#[derive(Debug, Clone)]
pub struct SessionMeta {
    pub id: String,
    pub cwd: String,
    pub branch: Option<String>,
    pub source: String,
    pub model_provider: String,
}

/// A subagent spawn extracted from a `collab_agent_spawn_end` event.
#[derive(Debug, Clone)]
pub struct SubagentSpawn {
    pub nickname: String,
    pub role: String,
    pub thread_id: String,
    pub prompt: String,
}

/// A tool call extracted from a `function_call` response item.
#[derive(Debug, Clone)]
pub struct ToolCallInfo {
    pub name: String,
    pub arguments: String,
}

/// Inferred status of a Codex rollout based on its events.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodexInferredStatus {
    Running,
    Done,
    Error,
    Unknown,
}

/// An entry from the Codex session index JSONL file.
#[derive(Debug, Clone)]
pub struct SessionIndexEntry {
    pub id: String,
    pub thread_name: String,
    pub updated_at: String,
}

/// Parsed source field info — distinguishes CLI launches from subagent spawns.
#[derive(Debug, Clone)]
pub struct SourceInfo {
    pub is_subagent: bool,
    pub parent_thread_id: Option<String>,
    pub agent_nickname: Option<String>,
    pub agent_role: Option<String>,
}

/// A Codex thread from the SQLite database.
#[derive(Debug, Clone)]
pub struct CodexThread {
    pub id: String,
    pub title: Option<String>,
    pub cwd: Option<String>,
    pub source: Option<String>,
    pub model: Option<String>,
    pub git_branch: Option<String>,
    pub agent_nickname: Option<String>,
    pub agent_role: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub archived: i32,
    pub tokens_used: Option<i64>,
    pub rollout_path: Option<String>,
}

/// A parent→child spawn edge from the SQLite database.
#[derive(Debug, Clone)]
pub struct SpawnEdge {
    pub parent_thread_id: String,
    pub child_thread_id: String,
    pub status: String,
}

/// Snapshot classification for an observed Codex thread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodexThreadClassification {
    TopLevel,
    Child { parent_thread_id: Option<String> },
}

/// An observed Codex thread enriched with identity and classification.
#[derive(Debug, Clone)]
pub struct ObservedCodexThread {
    pub thread: CodexThread,
    pub agent_id: AgentId,
    pub pid: Option<u32>,
    pub classification: CodexThreadClassification,
}

/// Grouped Codex observation snapshot for the current polling pass.
#[derive(Debug, Clone, Default)]
pub struct CodexSnapshot {
    pub top_level_threads: Vec<ObservedCodexThread>,
    pub child_threads: Vec<ObservedCodexThread>,
}

/// Snapshot of the active top-level tool call for a Codex session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedToolCall {
    pub name: String,
    pub args_summary: Option<String>,
}

/// Snapshot of a child thread represented as a subagent under its parent session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedCodexSubagent {
    pub thread_id: String,
    pub name: String,
    pub description: Option<String>,
    pub status: PrimaryStatus,
}

/// Event-facing snapshot for a top-level Codex session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedCodexSession {
    pub agent_id: AgentId,
    pub pid: Option<u32>,
    pub cwd: PathBuf,
    pub branch: Option<String>,
    pub started_at: Option<DateTime<Local>>,
    pub status: PrimaryStatus,
    pub final_output: Option<String>,
    pub latest_tool_call: Option<ObservedToolCall>,
    pub subagents: Vec<ObservedCodexSubagent>,
}

/// Reconciled Codex snapshot keyed by top-level session/thread identity.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CodexEventSnapshot {
    pub sessions: Vec<ObservedCodexSession>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RolloutThreadSummary {
    status: PrimaryStatus,
    final_output: Option<String>,
    latest_tool_call: Option<ObservedToolCall>,
    spawned_subagents: Vec<ObservedCodexSubagent>,
}

// ── JSONL Parsing ─────────────────────────────────────────────────

/// Parse a Codex rollout JSONL string into a list of events.
///
/// Each line is a JSON object with `timestamp`, `type`, and `payload` fields.
pub fn parse_rollout(content: &str) -> Vec<RolloutEvent> {
    let mut events = Vec::new();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let obj: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(e) => {
                debug!("failed to parse rollout line: {e}");
                continue;
            }
        };

        let timestamp = obj
            .get("timestamp")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let event_type = obj
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let payload = obj.get("payload").cloned().unwrap_or(Value::Null);

        events.push(RolloutEvent {
            timestamp,
            event_type,
            payload,
        });
    }

    events
}

/// Parse a Codex session index JSONL string into a list of entries.
///
/// Each line is a JSON object with `id`, `thread_name`, and `updated_at`.
pub fn parse_session_index(content: &str) -> Vec<SessionIndexEntry> {
    let mut entries = Vec::new();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let obj: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(e) => {
                debug!("failed to parse session index line: {e}");
                continue;
            }
        };

        let id = match obj.get("id").and_then(|v| v.as_str()) {
            Some(s) => s.to_string(),
            None => continue,
        };

        let thread_name = obj
            .get("thread_name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let updated_at = obj
            .get("updated_at")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        entries.push(SessionIndexEntry {
            id,
            thread_name,
            updated_at,
        });
    }

    entries
}

// ── Extraction Functions ──────────────────────────────────────────

/// Extract session metadata from the first `session_meta` event in a rollout.
pub fn extract_session_meta(events: &[RolloutEvent]) -> Option<SessionMeta> {
    let event = events.iter().find(|e| e.event_type == "session_meta")?;

    let payload = &event.payload;

    let id = payload.get("id")?.as_str()?.to_string();
    let cwd = payload.get("cwd")?.as_str()?.to_string();
    let source = payload
        .get("source")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let model_provider = payload
        .get("model_provider")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let branch = payload
        .get("git")
        .and_then(|g| g.get("branch"))
        .and_then(|v| v.as_str())
        .map(String::from);

    Some(SessionMeta {
        id,
        cwd,
        branch,
        source,
        model_provider,
    })
}

/// Extract subagent spawn events from a rollout.
///
/// Looks for `event_msg` entries whose payload type is `collab_agent_spawn_end`.
pub fn extract_subagent_spawns(events: &[RolloutEvent]) -> Vec<SubagentSpawn> {
    events
        .iter()
        .filter(|e| e.event_type == "event_msg")
        .filter(|e| {
            e.payload.get("type").and_then(|v| v.as_str()) == Some("collab_agent_spawn_end")
        })
        .filter_map(|e| {
            let p = &e.payload;
            Some(SubagentSpawn {
                nickname: p.get("new_agent_nickname")?.as_str()?.to_string(),
                role: p.get("new_agent_role")?.as_str()?.to_string(),
                thread_id: p.get("new_thread_id")?.as_str()?.to_string(),
                prompt: p
                    .get("prompt")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
            })
        })
        .collect()
}

/// Extract tool calls from a rollout.
///
/// Looks for `response_item` entries whose payload type is `function_call`.
pub fn extract_tool_calls(events: &[RolloutEvent]) -> Vec<ToolCallInfo> {
    events
        .iter()
        .filter(|e| e.event_type == "response_item")
        .filter(|e| e.payload.get("type").and_then(|v| v.as_str()) == Some("function_call"))
        .filter_map(|e| {
            let p = &e.payload;
            Some(ToolCallInfo {
                name: p.get("name")?.as_str()?.to_string(),
                arguments: p
                    .get("arguments")
                    .and_then(|v| v.as_str())
                    .unwrap_or("{}")
                    .to_string(),
            })
        })
        .collect()
}

// ── Status Inference ──────────────────────────────────────────────

/// Infer the status of a Codex rollout from its events.
///
/// - `task_complete` present → Done
/// - Any event_msg with type containing "error" → Error
/// - `task_started` present but no `task_complete` → Running
/// - Otherwise → Unknown
pub fn infer_rollout_status(events: &[RolloutEvent]) -> CodexInferredStatus {
    let has_task_complete = events.iter().any(|e| {
        e.event_type == "event_msg"
            && e.payload.get("type").and_then(|v| v.as_str()) == Some("task_complete")
    });

    if has_task_complete {
        return CodexInferredStatus::Done;
    }

    let has_error = events.iter().any(|e| {
        e.event_type == "event_msg"
            && e.payload
                .get("type")
                .and_then(|v| v.as_str())
                .is_some_and(|t| t.contains("error"))
    });

    if has_error {
        return CodexInferredStatus::Error;
    }

    let has_task_started = events.iter().any(|e| {
        e.event_type == "event_msg"
            && e.payload.get("type").and_then(|v| v.as_str()) == Some("task_started")
    });

    if has_task_started {
        return CodexInferredStatus::Running;
    }

    CodexInferredStatus::Unknown
}

// ── Source Field Parsing ──────────────────────────────────────────

/// Parse the `source` field from a Codex session meta event.
///
/// If the source starts with `{`, it's parsed as JSON to extract subagent
/// spawn information. Otherwise, it's treated as a simple string (e.g., "cli").
pub fn parse_source_field(source: &str) -> SourceInfo {
    if source.starts_with('{') {
        if let Ok(value) = serde_json::from_str::<Value>(source) {
            if let Some(spawn) = value.get("subagent").and_then(|s| s.get("thread_spawn")) {
                return SourceInfo {
                    is_subagent: true,
                    parent_thread_id: spawn
                        .get("parent_thread_id")
                        .and_then(|v| v.as_str())
                        .map(String::from),
                    agent_nickname: spawn
                        .get("agent_nickname")
                        .and_then(|v| v.as_str())
                        .map(String::from),
                    agent_role: spawn
                        .get("agent_role")
                        .and_then(|v| v.as_str())
                        .map(String::from),
                };
            }
        }
    }

    SourceInfo {
        is_subagent: false,
        parent_thread_id: None,
        agent_nickname: None,
        agent_role: None,
    }
}

// ── SQLite Queries ────────────────────────────────────────────────

/// Query Codex's SQLite database for active (non-archived) threads.
///
/// Opens the database in read-only mode and returns up to 50 threads
/// ordered by most recently updated first.
pub fn query_codex_threads(db_path: &Path) -> anyhow::Result<Vec<CodexThread>> {
    let conn = rusqlite::Connection::open_with_flags(
        db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;

    let mut stmt = conn.prepare(
        "SELECT id, title, cwd, source, model, git_branch,
                agent_nickname, agent_role, created_at, updated_at,
                archived, tokens_used, rollout_path
         FROM threads
         WHERE archived = 0
         ORDER BY updated_at DESC
         LIMIT 50",
    )?;

    let threads = stmt
        .query_map([], |row| {
            Ok(CodexThread {
                id: row.get(0)?,
                title: row.get(1)?,
                cwd: row.get(2)?,
                source: row.get(3)?,
                model: row.get(4)?,
                git_branch: row.get(5)?,
                agent_nickname: row.get(6)?,
                agent_role: row.get(7)?,
                created_at: row.get(8)?,
                updated_at: row.get(9)?,
                archived: row.get(10)?,
                tokens_used: row.get(11)?,
                rollout_path: row.get(12)?,
            })
        })?
        .filter_map(|r| match r {
            Ok(t) => Some(t),
            Err(e) => {
                debug!("failed to parse codex thread row: {e}");
                None
            }
        })
        .collect();

    Ok(threads)
}

/// Query spawn edges (parent→child thread relationships) from the SQLite database.
pub fn query_spawn_edges(db_path: &Path, parent_thread_id: &str) -> anyhow::Result<Vec<SpawnEdge>> {
    let conn = rusqlite::Connection::open_with_flags(
        db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;

    let mut stmt = conn.prepare(
        "SELECT parent_thread_id, child_thread_id, status
         FROM thread_spawn_edges
         WHERE parent_thread_id = ?1",
    )?;

    let edges = stmt
        .query_map(rusqlite::params![parent_thread_id], |row| {
            Ok(SpawnEdge {
                parent_thread_id: row.get(0)?,
                child_thread_id: row.get(1)?,
                status: row.get(2)?,
            })
        })?
        .filter_map(|r| match r {
            Ok(e) => Some(e),
            Err(e) => {
                debug!("failed to parse spawn edge row: {e}");
                None
            }
        })
        .collect();

    Ok(edges)
}

/// Query all spawn edges (parent→child thread relationships) from the SQLite database.
pub fn query_all_spawn_edges(db_path: &Path) -> anyhow::Result<Vec<SpawnEdge>> {
    let conn = rusqlite::Connection::open_with_flags(
        db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;

    let mut stmt = conn.prepare(
        "SELECT parent_thread_id, child_thread_id, status
         FROM thread_spawn_edges",
    )?;

    let edges = stmt
        .query_map([], |row| {
            Ok(SpawnEdge {
                parent_thread_id: row.get(0)?,
                child_thread_id: row.get(1)?,
                status: row.get(2)?,
            })
        })?
        .filter_map(|r| match r {
            Ok(e) => Some(e),
            Err(e) => {
                debug!("failed to parse spawn edge row: {e}");
                None
            }
        })
        .collect();

    Ok(edges)
}

// ── Helpers ───────────────────────────────────────────────────────

/// Map a CodexInferredStatus to a PrimaryStatus.
fn codex_to_primary(status: CodexInferredStatus) -> PrimaryStatus {
    match status {
        CodexInferredStatus::Running => PrimaryStatus::Running,
        CodexInferredStatus::Done => PrimaryStatus::Done,
        CodexInferredStatus::Error => PrimaryStatus::Error,
        CodexInferredStatus::Unknown => PrimaryStatus::Running,
    }
}

fn codex_pid_for_thread(registry: &AgentRegistry, cwd: &Path) -> Option<u32> {
    match registry.find_pids(AgentTool::Codex, cwd).as_slice() {
        [pid] => Some(*pid),
        _ => None,
    }
}

fn thread_is_recent_enough_for_pid_binding(thread: &CodexThread) -> bool {
    let thread_timestamp = thread.created_at.max(thread.updated_at);
    if thread_timestamp <= 0 {
        return false;
    }

    let now = Local::now().timestamp();
    let age = now.saturating_sub(thread_timestamp);
    age <= CONSERVATIVE_PID_BINDING_WINDOW_SECS
}

fn classify_codex_thread(
    thread: &CodexThread,
    edge_parent_by_child: &HashMap<&str, &str>,
) -> CodexThreadClassification {
    let source_info = thread
        .source
        .as_deref()
        .map(parse_source_field)
        .unwrap_or(SourceInfo {
            is_subagent: false,
            parent_thread_id: None,
            agent_nickname: None,
            agent_role: None,
        });
    let parent_thread_id = source_info.parent_thread_id.or_else(|| {
        edge_parent_by_child
            .get(thread.id.as_str())
            .map(|id| (*id).to_string())
    });

    if source_info.is_subagent || parent_thread_id.is_some() {
        CodexThreadClassification::Child { parent_thread_id }
    } else {
        CodexThreadClassification::TopLevel
    }
}

pub fn build_codex_snapshot(
    threads: &[CodexThread],
    spawn_edges: &[SpawnEdge],
    registry: &AgentRegistry,
) -> CodexSnapshot {
    let edge_parent_by_child: HashMap<&str, &str> = spawn_edges
        .iter()
        .map(|edge| {
            (
                edge.child_thread_id.as_str(),
                edge.parent_thread_id.as_str(),
            )
        })
        .collect();
    let classified_threads = threads
        .iter()
        .map(|thread| {
            (
                thread.clone(),
                classify_codex_thread(thread, &edge_parent_by_child),
            )
        })
        .collect::<Vec<_>>();
    let mut top_level_thread_count_by_cwd: HashMap<String, usize> = HashMap::new();
    for (thread, classification) in &classified_threads {
        if matches!(classification, CodexThreadClassification::TopLevel) {
            if let Some(cwd) = thread.cwd.as_deref() {
                *top_level_thread_count_by_cwd
                    .entry(cwd.to_string())
                    .or_default() += 1;
            }
        }
    }

    let mut snapshot = CodexSnapshot::default();

    for (thread, classification) in classified_threads {
        let pid = match &classification {
            CodexThreadClassification::TopLevel if thread_is_recent_enough_for_pid_binding(&thread) => thread
                .cwd
                .as_deref()
                .and_then(|cwd| {
                    (top_level_thread_count_by_cwd.get(cwd) == Some(&1))
                        .then(|| codex_pid_for_thread(registry, Path::new(cwd)))
                        .flatten()
                }),
            CodexThreadClassification::Child { .. } => None,
            CodexThreadClassification::TopLevel => None,
        };
        let observed = ObservedCodexThread {
            agent_id: AgentId::session(AgentTool::Codex, thread.id.clone()),
            pid,
            classification: classification.clone(),
            thread,
        };

        match classification {
            CodexThreadClassification::TopLevel => snapshot.top_level_threads.push(observed),
            CodexThreadClassification::Child { .. } => snapshot.child_threads.push(observed),
        }
    }

    snapshot
}

fn load_spawn_edges_snapshot(db_path: &Path) -> Vec<SpawnEdge> {
    match query_all_spawn_edges(db_path) {
        Ok(edges) => edges,
        Err(error) => {
            debug!("failed to load codex spawn edges: {error}");
            Vec::new()
        }
    }
}

fn snapshot_child_threads_by_parent(
    snapshot: &CodexSnapshot,
) -> HashMap<String, Vec<ObservedCodexThread>> {
    let mut grouped = HashMap::new();

    for child in &snapshot.child_threads {
        if let CodexThreadClassification::Child {
            parent_thread_id: Some(parent_thread_id),
        } = &child.classification
        {
            grouped
                .entry(parent_thread_id.clone())
                .or_insert_with(Vec::new)
                .push(child.clone());
        }
    }

    grouped
}

fn summarize_tool_arguments(arguments: &str) -> Option<String> {
    if arguments == "{}" {
        None
    } else if arguments.len() > 100 {
        Some(arguments[..100].to_string())
    } else {
        Some(arguments.to_string())
    }
}

fn extract_final_output_text(events: &[RolloutEvent]) -> Option<String> {
    if let Some(task_complete) = events.iter().rev().find(|event| {
        event.event_type == "event_msg"
            && event.payload.get("type").and_then(|value| value.as_str()) == Some("task_complete")
    }) {
        return task_complete
            .payload
            .get("last_agent_message")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(str::to_string);
    }

    events
        .iter()
        .rev()
        .find_map(|event| {
            if event.event_type != "response_item" {
                return None;
            }

            let payload = &event.payload;
            if payload.get("type").and_then(|value| value.as_str()) != Some("message") {
                return None;
            }

            payload
                .get("content")
                .and_then(|value| value.as_array())
                .and_then(|content| {
                    let parts = content
                        .iter()
                        .filter(|item| {
                            item.get("type").and_then(|value| value.as_str())
                                == Some("output_text")
                        })
                        .filter_map(|item| item.get("text").and_then(|value| value.as_str()))
                        .map(str::trim)
                        .filter(|text| !text.is_empty())
                        .collect::<Vec<_>>();
                    (!parts.is_empty()).then(|| parts.join("\n"))
                })
        })
}

fn looks_like_review_output(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return false;
    }

    let lower = trimmed.to_ascii_lowercase();
    [
        "### issues",
        "#### critical",
        "requested changes",
        "review findings",
        "merge-readiness assessment",
        "review assessment",
        "audit findings",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
}

pub fn normalize_codex_session(
    session: &ObservedCodexSession,
    final_output: Option<&str>,
) -> NormalizedState {
    let has_meaningful_final_output = final_output
        .map(str::trim)
        .is_some_and(|output| !output.is_empty());

    if session.status == PrimaryStatus::Done && final_output.is_some_and(looks_like_review_output) {
        return NormalizedState::new(
            NormalizedStatus::NeedsInput,
            Some(NormalizedReason::Review),
        );
    }

    if session
        .subagents
        .iter()
        .any(|subagent| subagent.status == PrimaryStatus::Running)
    {
        return NormalizedState::new(
            NormalizedStatus::Working,
            Some(NormalizedReason::Subagent),
        );
    }

    match session.status {
        PrimaryStatus::Error => {
            NormalizedState::new(NormalizedStatus::Blocked, Some(NormalizedReason::Error))
        }
        PrimaryStatus::Done if has_meaningful_final_output => NormalizedState::new(
            NormalizedStatus::Done,
            Some(NormalizedReason::ReadyForNextTask),
        ),
        PrimaryStatus::Done => NormalizedState::new(NormalizedStatus::Unknown, None),
        PrimaryStatus::Running => {
            if session.latest_tool_call.is_some() {
                NormalizedState::new(
                    NormalizedStatus::Working,
                    Some(NormalizedReason::ToolCall),
                )
            } else {
                NormalizedState::new(NormalizedStatus::Working, None)
            }
        }
        PrimaryStatus::Waiting => {
            if session.latest_tool_call.is_some() {
                NormalizedState::new(
                    NormalizedStatus::Working,
                    Some(NormalizedReason::ToolCall),
                )
            } else {
                NormalizedState::new(NormalizedStatus::Unknown, None)
            }
        }
        PrimaryStatus::Exited => NormalizedState::new(NormalizedStatus::Unknown, None),
    }
}

fn rollout_summary_for_thread(thread: &CodexThread) -> RolloutThreadSummary {
    let mut summary = RolloutThreadSummary {
        status: PrimaryStatus::Waiting,
        final_output: None,
        latest_tool_call: None,
        spawned_subagents: Vec::new(),
    };

    if let Some(ref rollout_path) = thread.rollout_path {
        let path = PathBuf::from(rollout_path);
        if path.exists() {
            if let Ok(content) = std::fs::read_to_string(&path) {
                let events = parse_rollout(&content);
                summary.status = codex_to_primary(infer_rollout_status(&events));
                summary.final_output = extract_final_output_text(&events);
                summary.latest_tool_call =
                    extract_tool_calls(&events)
                        .last()
                        .map(|tool| ObservedToolCall {
                            name: tool.name.clone(),
                            args_summary: summarize_tool_arguments(&tool.arguments),
                        });
                summary.spawned_subagents = extract_subagent_spawns(&events)
                    .into_iter()
                    .map(|spawn| {
                        let thread_id = spawn.thread_id;
                        ObservedCodexSubagent {
                            name: spawn.nickname,
                            thread_id: thread_id.clone(),
                            description: Some(thread_id),
                            status: PrimaryStatus::Running,
                        }
                    })
                    .collect();
            }
        }
    }

    summary
}

fn child_thread_subagent_metadata(child: &ObservedCodexThread) -> (String, Option<String>) {
    let source_info = child
        .thread
        .source
        .as_deref()
        .map(parse_source_field)
        .unwrap_or(SourceInfo {
            is_subagent: false,
            parent_thread_id: None,
            agent_nickname: None,
            agent_role: None,
        });
    let name = child
        .thread
        .agent_nickname
        .clone()
        .or(source_info.agent_nickname)
        .unwrap_or_else(|| child.thread.id.clone());

    (name, Some(child.thread.id.clone()))
}

fn child_thread_status(child: &ObservedCodexThread, edge_status: Option<&str>) -> PrimaryStatus {
    if let Some(ref rollout_path) = child.thread.rollout_path {
        let rp = PathBuf::from(rollout_path);
        if rp.exists() {
            if let Ok(content) = std::fs::read_to_string(&rp) {
                let inferred = codex_to_primary(infer_rollout_status(&parse_rollout(&content)));
                if inferred == PrimaryStatus::Done {
                    return PrimaryStatus::Done;
                }
            }
        }
    }

    match edge_status {
        Some("completed") => PrimaryStatus::Done,
        _ => PrimaryStatus::Running,
    }
}

fn observed_subagents_for_parent(
    parent: &ObservedCodexThread,
    child_threads_by_parent: &HashMap<String, Vec<ObservedCodexThread>>,
    spawn_status_by_child: &HashMap<&str, &str>,
    rollout_subagents: Vec<ObservedCodexSubagent>,
) -> Vec<ObservedCodexSubagent> {
    let mut subagents_by_thread_id: HashMap<String, ObservedCodexSubagent> =
        child_threads_by_parent
            .get(&parent.thread.id)
            .into_iter()
            .flatten()
            .map(|child| {
                let (name, description) = child_thread_subagent_metadata(child);
                let status = child_thread_status(
                    child,
                    spawn_status_by_child.get(child.thread.id.as_str()).copied(),
                );

                ObservedCodexSubagent {
                    thread_id: child.thread.id.clone(),
                    name,
                    description,
                    status,
                }
            })
            .map(|subagent| (subagent.thread_id.clone(), subagent))
            .collect();

    for subagent in rollout_subagents {
        subagents_by_thread_id
            .entry(subagent.thread_id.clone())
            .or_insert(subagent);
    }

    let mut subagents = subagents_by_thread_id.into_values().collect::<Vec<_>>();
    subagents.sort_by(|a, b| a.thread_id.cmp(&b.thread_id));
    subagents
}

pub fn build_codex_event_snapshot(
    snapshot: &CodexSnapshot,
    spawn_edges: &[SpawnEdge],
) -> CodexEventSnapshot {
    let child_threads_by_parent = snapshot_child_threads_by_parent(snapshot);
    let spawn_status_by_child: HashMap<&str, &str> = spawn_edges
        .iter()
        .map(|edge| (edge.child_thread_id.as_str(), edge.status.as_str()))
        .collect();
    let mut sessions = snapshot
        .top_level_threads
        .iter()
        .map(|observed| {
            let cwd = observed
                .thread
                .cwd
                .as_deref()
                .map(PathBuf::from)
                .unwrap_or_default();
            let rollout_summary = rollout_summary_for_thread(&observed.thread);

            ObservedCodexSession {
                agent_id: observed.agent_id.clone(),
                pid: observed.pid,
                cwd,
                branch: observed.thread.git_branch.clone(),
                started_at: from_epoch_secs(observed.thread.created_at),
                status: rollout_summary.status,
                final_output: rollout_summary.final_output,
                latest_tool_call: rollout_summary.latest_tool_call,
                subagents: observed_subagents_for_parent(
                    observed,
                    &child_threads_by_parent,
                    &spawn_status_by_child,
                    rollout_summary.spawned_subagents,
                ),
            }
        })
        .collect::<Vec<_>>();
    sessions.sort_by(|a, b| a.agent_id.session_id().cmp(&b.agent_id.session_id()));

    CodexEventSnapshot { sessions }
}

pub fn reconcile_codex_snapshots(
    previous: Option<&CodexEventSnapshot>,
    current: &CodexEventSnapshot,
) -> Vec<AgentEvent> {
    let previous_sessions: HashMap<&str, &ObservedCodexSession> = previous
        .into_iter()
        .flat_map(|snapshot| snapshot.sessions.iter())
        .filter_map(|session| session.agent_id.session_id().map(|id| (id, session)))
        .collect();
    let mut events = Vec::new();

    for session in &current.sessions {
        let Some(session_id) = session.agent_id.session_id() else {
            continue;
        };
        let previous_session = previous_sessions.get(session_id).copied();

        if previous_session.is_none() {
            events.push(AgentEvent::Discovered {
                id: session.agent_id.clone(),
                pid: session.pid,
                cwd: session.cwd.clone(),
                started_at: session.started_at,
            });
        }

        let session_info_changed = previous_session.is_none_or(|previous_session| {
            previous_session.pid != session.pid
                || previous_session.branch != session.branch
                || previous_session.started_at != session.started_at
        });
        if session_info_changed {
            events.push(AgentEvent::SessionInfo {
                id: session.agent_id.clone(),
                pid: session.pid,
                session_id: session_id.to_string(),
                branch: session.branch.clone(),
                is_worktree: false,
                started_at: session.started_at,
            });
        }

        let previous_status = previous_session.map(|session| session.status);
        if previous_status != Some(session.status)
            && !(previous_status.is_none() && session.status == PrimaryStatus::Running)
        {
            events.push(AgentEvent::StatusChanged {
                id: session.agent_id.clone(),
                pid: session.pid,
                old: previous_status.unwrap_or(PrimaryStatus::Running),
                new: session.status,
            });
        }

        let normalized_state =
            normalize_codex_session(session, session.final_output.as_deref());
        let previous_normalized_state = previous_session
            .map(|previous_session| {
                normalize_codex_session(previous_session, previous_session.final_output.as_deref())
            });
        if previous_normalized_state.as_ref() != Some(&normalized_state) {
            events.push(AgentEvent::NormalizedStateChanged {
                id: session.agent_id.clone(),
                pid: session.pid,
                state: normalized_state,
            });
        }

        let previous_tool_call =
            previous_session.and_then(|session| session.latest_tool_call.as_ref());
        match (previous_tool_call, session.latest_tool_call.as_ref()) {
            (Some(previous_tool_call), Some(current_tool_call))
                if previous_tool_call != current_tool_call =>
            {
                events.push(AgentEvent::ToolCallCompleted {
                    id: session.agent_id.clone(),
                    pid: session.pid,
                    tool_name: previous_tool_call.name.clone(),
                });
                events.push(AgentEvent::ToolCallStarted {
                    id: session.agent_id.clone(),
                    pid: session.pid,
                    tool_name: current_tool_call.name.clone(),
                    args_summary: current_tool_call.args_summary.clone(),
                });
            }
            (Some(previous_tool_call), None) => {
                events.push(AgentEvent::ToolCallCompleted {
                    id: session.agent_id.clone(),
                    pid: session.pid,
                    tool_name: previous_tool_call.name.clone(),
                });
            }
            (None, Some(current_tool_call)) => {
                events.push(AgentEvent::ToolCallStarted {
                    id: session.agent_id.clone(),
                    pid: session.pid,
                    tool_name: current_tool_call.name.clone(),
                    args_summary: current_tool_call.args_summary.clone(),
                });
            }
            _ => {}
        }

        let previous_subagents: HashMap<&str, &ObservedCodexSubagent> = previous_session
            .into_iter()
            .flat_map(|session| session.subagents.iter())
            .map(|subagent| (subagent.thread_id.as_str(), subagent))
            .collect();

        for subagent in &session.subagents {
            match previous_subagents.get(subagent.thread_id.as_str()).copied() {
                None => {
                    events.push(AgentEvent::SubagentSpawned {
                        id: session.agent_id.clone(),
                        pid: session.pid,
                        subagent: crate::types::Subagent {
                            name: subagent.name.clone(),
                            status: subagent.status,
                            description: subagent.description.clone(),
                            started_at: Local::now(),
                            ended_at: (subagent.status == PrimaryStatus::Done).then(Local::now),
                        },
                    });
                }
                Some(previous_subagent)
                    if previous_subagent.status != PrimaryStatus::Done
                        && subagent.status == PrimaryStatus::Done =>
                {
                    events.push(AgentEvent::SubagentCompleted {
                        id: session.agent_id.clone(),
                        pid: session.pid,
                        subagent_name: subagent.thread_id.clone(),
                    });
                }
                _ => {}
            }
        }
    }

    events
}

// ── Detector Loop ─────────────────────────────────────────────────

/// Run the Codex detector polling loop.
///
/// Watches the Codex data directory for state changes by polling the
/// SQLite database at `codex_data_dir/state_5.sqlite`.
pub async fn run_codex_detector(
    tx: mpsc::Sender<AgentEvent>,
    codex_data_dir: PathBuf,
    poll_interval: Duration,
    registry: AgentRegistry,
) -> anyhow::Result<()> {
    let db_path = codex_data_dir.join("state_5.sqlite");
    let mut previous_snapshot: Option<CodexEventSnapshot> = None;

    loop {
        if db_path.exists() {
            let db = db_path.clone();
            let registry = registry.clone();
            let result = tokio::task::spawn_blocking(move || {
                let threads = query_codex_threads(&db)?;
                let edges = load_spawn_edges_snapshot(&db);
                let snapshot = build_codex_snapshot(&threads, &edges, &registry);
                Ok::<_, anyhow::Error>(build_codex_event_snapshot(&snapshot, &edges))
            })
            .await?;

            match result {
                Ok(current_snapshot) => {
                    let events =
                        reconcile_codex_snapshots(previous_snapshot.as_ref(), &current_snapshot);

                    for event in events {
                        let _ = tx.send(event).await;
                    }

                    previous_snapshot = Some(current_snapshot);
                }
                Err(e) => {
                    debug!("Failed to query Codex DB: {e}");
                }
            }
        }

        tokio::time::sleep(poll_interval).await;
    }
}

#[cfg(test)]
mod tests {
    use super::{
        build_codex_event_snapshot, codex_pid_for_thread, load_spawn_edges_snapshot,
        reconcile_codex_snapshots, snapshot_child_threads_by_parent, CodexEventSnapshot,
        CodexSnapshot, CodexThread, CodexThreadClassification, ObservedCodexSession,
        ObservedCodexSubagent, ObservedCodexThread,
    };
    use crate::detect::registry::AgentRegistry;
    use crate::event::AgentEvent;
    use crate::types::{AgentId, AgentTool, PrimaryStatus};
    use rusqlite::Connection;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn codex_pid_for_thread_returns_single_binding_only() {
        let registry = AgentRegistry::new();
        let cwd = PathBuf::from("/tmp/project");

        assert_eq!(codex_pid_for_thread(&registry, &cwd), None);

        registry.register(AgentTool::Codex, cwd.clone(), 101);
        assert_eq!(codex_pid_for_thread(&registry, &cwd), Some(101));

        registry.register(AgentTool::Codex, cwd.clone(), 202);
        assert_eq!(codex_pid_for_thread(&registry, &cwd), None);
    }

    #[test]
    fn snapshot_child_threads_by_parent_uses_classification() {
        let snapshot = CodexSnapshot {
            top_level_threads: vec![ObservedCodexThread {
                thread: CodexThread {
                    id: "parent".into(),
                    title: None,
                    cwd: Some("/tmp/project".into()),
                    source: Some("cli".into()),
                    model: None,
                    git_branch: None,
                    agent_nickname: None,
                    agent_role: None,
                    created_at: 1,
                    updated_at: 1,
                    archived: 0,
                    tokens_used: None,
                    rollout_path: None,
                },
                agent_id: AgentId::session(AgentTool::Codex, "parent"),
                pid: None,
                classification: CodexThreadClassification::TopLevel,
            }],
            child_threads: vec![
                ObservedCodexThread {
                    thread: CodexThread {
                        id: "child-a".into(),
                        title: None,
                        cwd: Some("/tmp/project".into()),
                        source: Some("cli".into()),
                        model: None,
                        git_branch: None,
                        agent_nickname: None,
                        agent_role: None,
                        created_at: 2,
                        updated_at: 2,
                        archived: 0,
                        tokens_used: None,
                        rollout_path: None,
                    },
                    agent_id: AgentId::session(AgentTool::Codex, "child-a"),
                    pid: None,
                    classification: CodexThreadClassification::Child {
                        parent_thread_id: Some("parent".into()),
                    },
                },
                ObservedCodexThread {
                    thread: CodexThread {
                        id: "orphan".into(),
                        title: None,
                        cwd: Some("/tmp/project".into()),
                        source: Some("cli".into()),
                        model: None,
                        git_branch: None,
                        agent_nickname: None,
                        agent_role: None,
                        created_at: 3,
                        updated_at: 3,
                        archived: 0,
                        tokens_used: None,
                        rollout_path: None,
                    },
                    agent_id: AgentId::session(AgentTool::Codex, "orphan"),
                    pid: None,
                    classification: CodexThreadClassification::Child {
                        parent_thread_id: None,
                    },
                },
            ],
        };

        let grouped = snapshot_child_threads_by_parent(&snapshot);

        assert_eq!(grouped.len(), 1);
        assert_eq!(grouped["parent"][0].thread.id, "child-a");
    }

    #[test]
    fn load_spawn_edges_snapshot_ignores_missing_table() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let db_path = std::env::temp_dir().join(format!("codex-missing-edges-{unique}.sqlite"));
        Connection::open(&db_path).unwrap();

        let edges = load_spawn_edges_snapshot(&db_path);

        assert!(edges.is_empty());

        std::fs::remove_file(db_path).unwrap();
    }

    #[test]
    fn build_codex_event_snapshot_includes_child_threads_as_subagents() {
        let snapshot = CodexSnapshot {
            top_level_threads: vec![ObservedCodexThread {
                thread: CodexThread {
                    id: "parent".into(),
                    title: None,
                    cwd: Some("/tmp/project".into()),
                    source: Some("cli".into()),
                    model: None,
                    git_branch: Some("main".into()),
                    agent_nickname: None,
                    agent_role: None,
                    created_at: 1,
                    updated_at: 1,
                    archived: 0,
                    tokens_used: None,
                    rollout_path: None,
                },
                agent_id: AgentId::session(AgentTool::Codex, "parent"),
                pid: Some(44),
                classification: CodexThreadClassification::TopLevel,
            }],
            child_threads: vec![ObservedCodexThread {
                thread: CodexThread {
                    id: "child-a".into(),
                    title: None,
                    cwd: Some("/tmp/project".into()),
                    source: Some("cli".into()),
                    model: None,
                    git_branch: None,
                    agent_nickname: Some("worker".into()),
                    agent_role: Some("worker".into()),
                    created_at: 2,
                    updated_at: 2,
                    archived: 0,
                    tokens_used: None,
                    rollout_path: None,
                },
                agent_id: AgentId::session(AgentTool::Codex, "child-a"),
                pid: None,
                classification: CodexThreadClassification::Child {
                    parent_thread_id: Some("parent".into()),
                },
            }],
        };

        let event_snapshot = build_codex_event_snapshot(&snapshot, &[]);

        assert_eq!(event_snapshot.sessions.len(), 1);
        assert_eq!(event_snapshot.sessions[0].subagents.len(), 1);
        assert_eq!(event_snapshot.sessions[0].subagents[0].thread_id, "child-a");
        assert_eq!(event_snapshot.sessions[0].subagents[0].name, "worker");
    }

    #[test]
    fn reconcile_codex_snapshots_emits_subagent_completion_delta() {
        let previous = CodexEventSnapshot {
            sessions: vec![ObservedCodexSession {
                agent_id: AgentId::session(AgentTool::Codex, "parent"),
                pid: Some(44),
                cwd: PathBuf::from("/tmp/project"),
                branch: Some("main".into()),
                started_at: crate::types::from_epoch_secs(1),
                status: PrimaryStatus::Running,
                final_output: None,
                latest_tool_call: None,
                subagents: vec![ObservedCodexSubagent {
                    thread_id: "child-a".into(),
                    name: "child-a".into(),
                    description: Some("worker".into()),
                    status: PrimaryStatus::Running,
                }],
            }],
        };
        let current = CodexEventSnapshot {
            sessions: vec![ObservedCodexSession {
                subagents: vec![ObservedCodexSubagent {
                    status: PrimaryStatus::Done,
                    ..previous.sessions[0].subagents[0].clone()
                }],
                ..previous.sessions[0].clone()
            }],
        };

        let events = reconcile_codex_snapshots(Some(&previous), &current);

        assert!(matches!(
            events.as_slice(),
            [AgentEvent::SubagentCompleted { subagent_name, .. }]
                if subagent_name == "child-a"
        ));
    }
}
