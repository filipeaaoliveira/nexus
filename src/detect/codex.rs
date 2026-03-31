use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::Value;
use tokio::sync::mpsc;
use tracing::debug;

use crate::event::AgentEvent;

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

        let payload = obj
            .get("payload")
            .cloned()
            .unwrap_or(Value::Null);

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
    let event = events
        .iter()
        .find(|e| e.event_type == "session_meta")?;

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
            e.payload
                .get("type")
                .and_then(|v| v.as_str())
                == Some("collab_agent_spawn_end")
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
        .filter(|e| {
            e.payload
                .get("type")
                .and_then(|v| v.as_str())
                == Some("function_call")
        })
        .filter_map(|e| {
            let p = &e.payload;
            Some(ToolCallInfo {
                name: p.get("name")?.as_str()?.to_string(),
                arguments: p
                    .get("arguments")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
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
            if let Some(spawn) = value
                .get("subagent")
                .and_then(|s| s.get("thread_spawn"))
            {
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
         FROM thread
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
pub fn query_spawn_edges(
    db_path: &Path,
    parent_thread_id: &str,
) -> anyhow::Result<Vec<SpawnEdge>> {
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

// ── Detector Loop ─────────────────────────────────────────────────

/// Run the Codex detector polling loop.
///
/// Watches the Codex data directory for state changes by polling the
/// SQLite database at `codex_data_dir/state_5.sqlite`.
///
/// Event emission is deferred to Task 6 when PID-to-session mapping
/// is available.
pub async fn run_codex_detector(
    _tx: mpsc::Sender<AgentEvent>,
    codex_data_dir: PathBuf,
    poll_interval: Duration,
) -> anyhow::Result<()> {
    let db_path = codex_data_dir.join("state_5.sqlite");

    loop {
        if db_path.exists() {
            match query_codex_threads(&db_path) {
                Ok(threads) => {
                    for thread in &threads {
                        debug!(
                            "Codex thread: {} title={:?} updated={}",
                            thread.id, thread.title, thread.updated_at
                        );
                        // Event emission will be added in Task 6
                        // when PID-to-session mapping is available.
                    }
                }
                Err(e) => {
                    debug!("Failed to query Codex DB: {e}");
                }
            }
        }

        tokio::time::sleep(poll_interval).await;
    }
}
