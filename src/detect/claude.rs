use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Result;
use serde_json::Value;
use tokio::sync::mpsc;
use tracing::warn;

use crate::detect::registry::AgentRegistry;
use crate::event::AgentEvent;
use crate::types::{
    from_rfc3339, Activity, AgentId, AgentTool, PrimaryStatus, TodoItem, TodoPriority, TodoStatus,
};

/// Seconds since last transcript write before we consider the agent to be
/// waiting for user input. Derived from transcript gap analysis: p90 = 25.6 s.
const CLAUDE_IDLE_SECS: u64 = 10;

// ── TranscriptEntry ───────────────────────────────────────────────

/// A parsed entry from a Claude transcript JSONL file.
#[derive(Debug, Clone)]
pub enum TranscriptEntry {
    User {
        timestamp: String,
        content: String,
    },
    ToolUse {
        timestamp: String,
        tool_name: String,
        tool_input: serde_json::Map<String, Value>,
    },
    ToolResult {
        timestamp: String,
        tool_name: String,
    },
}

// ── InferredStatus ────────────────────────────────────────────────

/// Status inferred from the last transcript entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InferredStatus {
    Running,
    Waiting,
    ToolCall,
    Unknown,
}

// ── ProjectSessionInfo ────────────────────────────────────────────

/// Information extracted from a Claude project session JSONL file.
#[derive(Debug, Clone, Default)]
pub struct ProjectSessionInfo {
    pub session_id: Option<String>,
    pub branch: Option<String>,
    pub cwd: Option<PathBuf>,
    pub model: Option<String>,
    pub has_internal_initiator: bool,
    pub started_at: Option<String>,
}

// ── parse_transcript ──────────────────────────────────────────────

/// Parse a Claude transcript JSONL string into a list of entries.
/// Each line is a JSON object with at least a "type" and "timestamp" field.
pub fn parse_transcript(content: &str) -> Vec<TranscriptEntry> {
    let mut entries = Vec::new();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let obj: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(e) => {
                warn!("failed to parse transcript line: {e}");
                continue;
            }
        };

        let entry_type = match obj.get("type").and_then(|v| v.as_str()) {
            Some(t) => t,
            None => continue,
        };

        let timestamp = obj
            .get("timestamp")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        match entry_type {
            "user" => {
                let content = obj
                    .get("content")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                entries.push(TranscriptEntry::User { timestamp, content });
            }
            "tool_use" => {
                let tool_name = obj
                    .get("tool_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let tool_input = obj
                    .get("tool_input")
                    .and_then(|v| v.as_object())
                    .cloned()
                    .unwrap_or_default();
                entries.push(TranscriptEntry::ToolUse {
                    timestamp,
                    tool_name,
                    tool_input,
                });
            }
            "tool_result" => {
                let tool_name = obj
                    .get("tool_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                entries.push(TranscriptEntry::ToolResult {
                    timestamp,
                    tool_name,
                });
            }
            _ => {
                // Ignore unknown entry types (progress, assistant, etc.)
            }
        }
    }

    entries
}

// ── infer_status_from_transcript ──────────────────────────────────

/// Infer the current status of a Claude session from its transcript entries.
///
/// - ToolUse (last) → ToolCall (tool is executing)
/// - ToolResult (last) → Running (agent is generating after receiving result)
/// - User (last) → Running (agent is processing user input)
/// - Empty → Unknown
pub fn infer_status_from_transcript(entries: &[TranscriptEntry]) -> InferredStatus {
    match entries.last() {
        Some(TranscriptEntry::ToolUse { .. }) => InferredStatus::ToolCall,
        Some(TranscriptEntry::ToolResult { .. }) => InferredStatus::Running,
        Some(TranscriptEntry::User { .. }) => InferredStatus::Running,
        None => InferredStatus::Unknown,
    }
}

// ── infer_activity_from_last_entry ────────────────────────────────

/// Infer the current `Activity` from the last transcript entry.
///
/// - ToolUse (last) → Activity::ToolCall  (a tool is executing right now)
/// - ToolResult (last) → Activity::Thinking (agent is generating after result)
/// - User (last) → Activity::Generating   (agent is responding to user input)
/// - Empty → None
pub fn infer_activity_from_last_entry(entries: &[TranscriptEntry]) -> Option<Activity> {
    match entries.last() {
        Some(TranscriptEntry::ToolUse { .. }) => Some(Activity::ToolCall),
        Some(TranscriptEntry::ToolResult { .. }) => Some(Activity::Thinking),
        Some(TranscriptEntry::User { content, .. }) => {
            if content
                .contains("[restore checkpointed session agent configuration after compaction]")
            {
                Some(Activity::Compacting)
            } else {
                Some(Activity::Generating)
            }
        }
        None => None,
    }
}

// ── extract_todos_from_transcript ─────────────────────────────────

/// Extract the current todo list from transcript entries.
///
/// Finds the last `todowrite` tool_use call and parses its `tool_input.todos`
/// array. Returns empty vec if no todowrite calls found.
pub fn extract_todos_from_transcript(entries: &[TranscriptEntry]) -> Vec<TodoItem> {
    // Iterate in reverse to find the last todowrite call
    for entry in entries.iter().rev() {
        if let TranscriptEntry::ToolUse {
            tool_name,
            tool_input,
            ..
        } = entry
        {
            if tool_name == "todowrite" {
                if let Some(todos_value) = tool_input.get("todos") {
                    if let Some(todos_array) = todos_value.as_array() {
                        return todos_array
                            .iter()
                            .filter_map(|item| {
                                let content = item.get("content")?.as_str()?.to_string();
                                let status = item
                                    .get("status")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("pending");
                                let priority = item
                                    .get("priority")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("medium");
                                Some(TodoItem {
                                    content,
                                    status: TodoStatus::parse_str(status),
                                    priority: TodoPriority::parse_str(priority),
                                })
                            })
                            .collect();
                    }
                }
                return Vec::new();
            }
        }
    }
    Vec::new()
}

// ── is_stale ──────────────────────────────────────────────────────

/// Returns true if `mtime` is older than `threshold_secs` seconds ago.
pub fn is_stale(mtime: std::time::SystemTime, threshold_secs: u64) -> bool {
    mtime
        .elapsed()
        .map(|e| e.as_secs() >= threshold_secs)
        .unwrap_or(false)
}

// ── parse_project_session ─────────────────────────────────────────

/// Parse a Claude project session JSONL file to extract session metadata.
///
/// Extracts session ID, git branch, working directory, model, and detects
/// internal initiator markers (subagent sessions).
pub fn parse_project_session(content: &str) -> ProjectSessionInfo {
    let mut info = ProjectSessionInfo::default();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let obj: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };

        // Capture the timestamp of the first entry as session start time
        if info.started_at.is_none() {
            if let Some(ts) = obj.get("timestamp").and_then(|v| v.as_str()) {
                info.started_at = Some(ts.to_string());
            }
        }

        // Extract sessionId from any entry that has it
        if info.session_id.is_none() {
            if let Some(sid) = obj.get("sessionId").and_then(|v| v.as_str()) {
                info.session_id = Some(sid.to_string());
            }
        }

        let entry_type = match obj.get("type").and_then(|v| v.as_str()) {
            Some(t) => t,
            None => continue,
        };

        match entry_type {
            "user" => {
                // Extract branch and cwd from user entries
                if let Some(branch) = obj.get("gitBranch").and_then(|v| v.as_str()) {
                    info.branch = Some(branch.to_string());
                }
                if let Some(cwd) = obj.get("cwd").and_then(|v| v.as_str()) {
                    info.cwd = Some(PathBuf::from(cwd));
                }

                // Check for internal initiator marker in message content
                if let Some(message) = obj.get("message") {
                    if let Some(content) = message.get("content").and_then(|v| v.as_str()) {
                        if content.contains("<!-- OMO_INTERNAL_INITIATOR -->") {
                            info.has_internal_initiator = true;
                        }
                    }
                }
            }
            "assistant" => {
                // Extract model from assistant entries
                if let Some(message) = obj.get("message") {
                    if let Some(model) = message.get("model").and_then(|v| v.as_str()) {
                        info.model = Some(model.to_string());
                    }
                }
            }
            _ => {}
        }
    }

    info
}

// ── inferred_to_primary ───────────────────────────────────────────

/// Map an inferred transcript status to a primary display status.
pub fn inferred_to_primary(inferred: InferredStatus) -> PrimaryStatus {
    match inferred {
        InferredStatus::Running => PrimaryStatus::Running,
        InferredStatus::Waiting => PrimaryStatus::Waiting,
        InferredStatus::ToolCall => PrimaryStatus::Running,
        InferredStatus::Unknown => PrimaryStatus::Running,
    }
}

// ── find_transcript_for_pid ───────────────────────────────────────

/// Find the most recently modified transcript file matching a session prefix.
///
/// Searches `transcripts_dir` for `.jsonl` files whose name starts with
/// `session_prefix` and returns the one with the latest modification time.
pub fn find_transcript_for_pid(transcripts_dir: &Path, session_prefix: &str) -> Option<PathBuf> {
    let read_dir = std::fs::read_dir(transcripts_dir).ok()?;

    let mut best: Option<(PathBuf, std::time::SystemTime)> = None;

    for entry in read_dir.flatten() {
        let path = entry.path();
        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };

        if !name.starts_with(session_prefix) || !name.ends_with(".jsonl") {
            continue;
        }

        let modified = match entry.metadata().and_then(|m| m.modified()) {
            Ok(t) => t,
            Err(_) => continue,
        };

        if best.as_ref().is_none_or(|(_, prev)| modified > *prev) {
            best = Some((path, modified));
        }
    }

    best.map(|(p, _)| p)
}

// ── State tracking ────────────────────────────────────────────────

/// Per-session emit state to avoid duplicate events.
#[derive(Debug, Clone, Default)]
struct SessionEmitState {
    last_status: Option<PrimaryStatus>,
    last_tool_call: Option<String>,
    emitted_subagent_tools: Vec<String>,
    session_info_sent: bool,
    entry_count: usize,
    last_mtime: Option<std::time::SystemTime>,
    pid: Option<u32>,
    last_todos: Vec<TodoItem>,
}

// ── run_claude_detector ───────────────────────────────────────────

/// Polling loop that watches Claude transcript and project session files.
///
/// Scans `claude_data_dir/projects/` for project session JSONL files to
/// extract CWD→session mappings, then reads transcript files to emit
/// enrichment events (status, tool calls, subagents).
pub async fn run_claude_detector(
    tx: mpsc::Sender<AgentEvent>,
    claude_data_dir: PathBuf,
    poll_interval: Duration,
    registry: AgentRegistry,
) -> Result<()> {
    let transcripts_dir = claude_data_dir.join("transcripts");
    let projects_dir = claude_data_dir.join("projects");

    // Track file sizes to detect new content
    let mut file_sizes: HashMap<PathBuf, u64> = HashMap::new();
    // Track emit state per session (keyed by session_id or transcript path)
    let mut session_states: HashMap<String, SessionEmitState> = HashMap::new();
    // Track last known mtime for all transcript files (for stale-file Waiting detection)
    let mut file_mtimes: HashMap<PathBuf, std::time::SystemTime> = HashMap::new();

    loop {
        // Phase 1: Scan project session files to build CWD→session_id map
        let mut cwd_to_session: HashMap<PathBuf, ProjectSessionInfo> = HashMap::new();

        if projects_dir.exists() {
            let pdir = projects_dir.clone();
            let project_scan = tokio::task::spawn_blocking(move || {
                let mut results = Vec::new();
                // Projects dir contains subdirs per project, each with session JSONL files
                if let Ok(project_dirs) = std::fs::read_dir(&pdir) {
                    for project_entry in project_dirs.flatten() {
                        let project_path = project_entry.path();
                        if !project_path.is_dir() {
                            continue;
                        }
                        if let Ok(session_files) = std::fs::read_dir(&project_path) {
                            for file_entry in session_files.flatten() {
                                let file_path = file_entry.path();
                                let name = match file_path.file_name().and_then(|n| n.to_str()) {
                                    Some(n) => n.to_string(),
                                    None => continue,
                                };
                                if !name.ends_with(".jsonl") {
                                    continue;
                                }
                                if let Ok(content) = std::fs::read_to_string(&file_path) {
                                    let info = parse_project_session(&content);
                                    if info.cwd.is_some() {
                                        results.push(info);
                                    }
                                }
                            }
                        }
                    }
                }
                results
            })
            .await?;

            for info in project_scan {
                if let Some(ref cwd) = info.cwd {
                    cwd_to_session.insert(cwd.clone(), info);
                }
            }
        }

        // Phase 2: Scan transcripts and emit enrichment events
        if transcripts_dir.exists() {
            let dir = transcripts_dir.clone();
            let sizes = file_sizes.clone();
            let scan_result = tokio::task::spawn_blocking(move || {
                let mut new_sizes = sizes;
                let mut results: Vec<(PathBuf, u64, Vec<TranscriptEntry>)> = Vec::new();
                let mut mtime_only: Vec<(PathBuf, std::time::SystemTime)> = Vec::new();

                if let Ok(read_dir) = std::fs::read_dir(&dir) {
                    for entry in read_dir.flatten() {
                        let file_path = entry.path();
                        let name = match file_path.file_name().and_then(|n| n.to_str()) {
                            Some(n) => n.to_string(),
                            None => continue,
                        };

                        if !name.ends_with(".jsonl") {
                            continue;
                        }

                        let metadata = match entry.metadata() {
                            Ok(m) => m,
                            Err(_) => continue,
                        };

                        let current_size = metadata.len();
                        let prev_size = new_sizes.get(&file_path).copied().unwrap_or(0);

                        let mtime = metadata
                            .modified()
                            .unwrap_or(std::time::SystemTime::UNIX_EPOCH);

                        if current_size > prev_size {
                            if let Ok(content) = std::fs::read_to_string(&file_path) {
                                let entries = parse_transcript(&content);
                                results.push((file_path.clone(), current_size, entries));
                            }
                            new_sizes.insert(file_path.clone(), current_size);
                        }

                        mtime_only.push((file_path, mtime));
                    }
                }

                (new_sizes, results, mtime_only)
            })
            .await?;

            let (new_sizes, results, mtime_updates) = scan_result;
            file_sizes = new_sizes;

            // Update the mtime map
            for (path, mtime) in mtime_updates {
                file_mtimes.insert(path, mtime);
            }

            for (file_path, _current_size, entries) in &results {
                // Try to find a matching CWD for this transcript via project sessions.
                // Transcript filenames may contain a session ID prefix that matches
                // a project session. For simplicity, we try matching all known CWDs
                // against the registry.
                let mut matched_pid: Option<u32> = None;
                let mut matched_session: Option<&ProjectSessionInfo> = None;

                for (cwd, session_info) in &cwd_to_session {
                    if let Some(pid) = registry.find_pid(AgentTool::Claude, cwd) {
                        // Check if this transcript belongs to this session by matching
                        // the session_id in the filename
                        if let Some(ref sid) = session_info.session_id {
                            if let Some(fname) = file_path.file_name().and_then(|n| n.to_str()) {
                                if fname.contains(sid) {
                                    matched_pid = Some(pid);
                                    matched_session = Some(session_info);
                                    break;
                                }
                            }
                        }
                        // Fallback: if we only have one Claude process, use it
                        if matched_pid.is_none() {
                            matched_pid = Some(pid);
                            matched_session = Some(session_info);
                        }
                    }
                }

                // If no project session match, try all known Claude PIDs
                if matched_pid.is_none() {
                    let claude_agents = registry.agents_for_tool(AgentTool::Claude);
                    if claude_agents.len() == 1 {
                        matched_pid = Some(claude_agents[0].1);
                    }
                }

                let pid = match matched_pid {
                    Some(p) => p,
                    None => continue, // no matching process discovered yet
                };

                let session_key = file_path.to_string_lossy().to_string();
                let state = session_states.entry(session_key).or_default();

                state.pid = Some(pid);

                // Emit SessionInfo once
                if !state.session_info_sent {
                    if let Some(session_info) = matched_session {
                        let session_started =
                            session_info.started_at.as_deref().and_then(from_rfc3339);
                        let _ = tx
                            .send(AgentEvent::SessionInfo {
                                id: AgentId::new(AgentTool::Claude, pid),
                                session_id: session_info.session_id.clone().unwrap_or_default(),
                                branch: session_info.branch.clone(),
                                is_worktree: false,
                                started_at: session_started,
                            })
                            .await;
                    }
                    state.session_info_sent = true;
                }

                // Only process new entries since last check
                if entries.len() <= state.entry_count {
                    continue;
                }

                // If we were Waiting and new entries arrived, restore Running
                if state.last_status == Some(PrimaryStatus::Waiting) {
                    let _ = tx
                        .send(AgentEvent::StatusChanged {
                            id: AgentId::new(AgentTool::Claude, pid),
                            old: PrimaryStatus::Waiting,
                            new: PrimaryStatus::Running,
                        })
                        .await;
                    state.last_status = Some(PrimaryStatus::Running);
                }

                let new_entries = &entries[state.entry_count..];
                state.entry_count = entries.len();
                state.last_mtime = file_mtimes.get(file_path).copied();

                // Infer status from transcript
                let inferred = infer_status_from_transcript(entries);
                let primary = inferred_to_primary(inferred);

                if state.last_status != Some(primary) {
                    let old = state.last_status.unwrap_or(PrimaryStatus::Running);
                    let _ = tx
                        .send(AgentEvent::StatusChanged {
                            id: AgentId::new(AgentTool::Claude, pid),
                            old,
                            new: primary,
                        })
                        .await;
                    state.last_status = Some(primary);
                }

                // Emit tool call events from new entries
                for entry in new_entries {
                    match entry {
                        TranscriptEntry::ToolUse {
                            timestamp: ref entry_timestamp,
                            tool_name,
                            tool_input,
                        } => {
                            // Check for subagent spawn (Task tool with subagent_type)
                            if tool_name == "task" {
                                if let Some(subagent_type) =
                                    tool_input.get("subagent_type").and_then(|v| v.as_str())
                                {
                                    let key = format!("{}:{}", tool_name, subagent_type);
                                    if !state.emitted_subagent_tools.contains(&key) {
                                        let description = tool_input
                                            .get("description")
                                            .and_then(|v| v.as_str())
                                            .map(String::from);
                                        let _ = tx
                                            .send(AgentEvent::SubagentSpawned {
                                                id: AgentId::new(AgentTool::Claude, pid),
                                                subagent: crate::types::Subagent {
                                                    name: subagent_type.to_string(),
                                                    status: PrimaryStatus::Running,
                                                    description,
                                                    started_at: from_rfc3339(entry_timestamp)
                                                        .unwrap_or_else(chrono::Local::now),
                                                    ended_at: None,
                                                },
                                            })
                                            .await;
                                        state.emitted_subagent_tools.push(key);
                                    }
                                    continue;
                                }
                            }

                            // Regular tool call
                            if state.last_tool_call.as_deref() != Some(tool_name) {
                                let args_summary = tool_input
                                    .get("command")
                                    .or_else(|| tool_input.get("filePath"))
                                    .or_else(|| tool_input.get("pattern"))
                                    .and_then(|v| v.as_str())
                                    .map(String::from);
                                let _ = tx
                                    .send(AgentEvent::ToolCallStarted {
                                        id: AgentId::new(AgentTool::Claude, pid),
                                        tool_name: tool_name.clone(),
                                        args_summary,
                                    })
                                    .await;
                                state.last_tool_call = Some(tool_name.clone());
                            }
                        }
                        TranscriptEntry::ToolResult { .. } => {
                            // ToolResult means the agent received the result and is about to think.
                            // Do NOT emit ToolCallCompleted here — that would set activity=Idle.
                            // Activity will be updated via ActivityChanged in Task 3.
                        }
                        _ => {}
                    }
                }

                // Emit ActivityChanged based on what the last entry implies
                let new_activity = infer_activity_from_last_entry(entries);
                let _ = tx
                    .send(AgentEvent::ActivityChanged {
                        id: AgentId::new(AgentTool::Claude, pid),
                        activity: new_activity,
                    })
                    .await;

                // Extract todos and emit if changed
                let todos = extract_todos_from_transcript(entries);
                if todos != state.last_todos {
                    let _ = tx
                        .send(AgentEvent::TodosUpdated {
                            id: AgentId::new(AgentTool::Claude, pid),
                            todos: todos.clone(),
                        })
                        .await;
                    state.last_todos = todos;
                }
            }

            // Staleness check: if any Running session's transcript file hasn't been
            // written for CLAUDE_IDLE_SECS, emit StatusChanged → Waiting.
            for (session_key, state) in session_states.iter_mut() {
                if state.last_status != Some(PrimaryStatus::Running) {
                    continue;
                }
                let path = PathBuf::from(session_key.as_str());
                let mtime = match file_mtimes.get(&path) {
                    Some(m) => *m,
                    None => continue,
                };
                if !is_stale(mtime, CLAUDE_IDLE_SECS) {
                    continue;
                }
                let pid = match state.pid {
                    Some(p) => p,
                    None => continue,
                };
                let old = state.last_status.unwrap_or(PrimaryStatus::Running);
                let _ = tx
                    .send(AgentEvent::StatusChanged {
                        id: AgentId::new(AgentTool::Claude, pid),
                        old,
                        new: PrimaryStatus::Waiting,
                    })
                    .await;
                let _ = tx
                    .send(AgentEvent::ActivityChanged {
                        id: AgentId::new(AgentTool::Claude, pid),
                        activity: None, // clear activity when Waiting
                    })
                    .await;
                state.last_status = Some(PrimaryStatus::Waiting);
            }
        }

        tokio::time::sleep(poll_interval).await;
    }
}
