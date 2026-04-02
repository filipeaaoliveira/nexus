use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::OptionalExtension;
use serde_json::Value;
use tokio::sync::mpsc;
use tracing::debug;

use crate::detect::registry::AgentRegistry;
use crate::event::AgentEvent;
use crate::types::{
    from_epoch_millis, Activity, AgentId, AgentTool, NormalizedReason, NormalizedState,
    NormalizedStatus, PrimaryStatus, TodoItem, TodoPriority, TodoStatus,
};

/// Parsed message data from OpenCode's SQLite `message.data` JSON.
#[derive(Debug, Clone)]
pub struct MessageData {
    pub role: String,
    pub agent: Option<String>,
    pub model_id: Option<String>,
    pub provider_id: Option<String>,
}

/// Parsed part data from OpenCode's SQLite `part.data` JSON.
#[derive(Debug, Clone)]
pub struct PartData {
    pub part_type: String,
    pub tool_name: Option<String>,
    pub tool_status: Option<String>,
    pub tool_input_summary: Option<String>,
    pub time_created: i64,
}

/// Parse message.data JSON column.
pub fn parse_message_data(json: &str) -> Option<MessageData> {
    let value: Value = serde_json::from_str(json).ok()?;
    let role = value.get("role")?.as_str()?.to_string();
    let agent = value
        .get("agent")
        .and_then(|v| v.as_str())
        .map(String::from);
    let model_id = value
        .get("modelID")
        .and_then(|v| v.as_str())
        .or_else(|| {
            value
                .get("model")
                .and_then(|m| m.get("modelID"))
                .and_then(|v| v.as_str())
        })
        .map(String::from);
    let provider_id = value
        .get("providerID")
        .and_then(|v| v.as_str())
        .or_else(|| {
            value
                .get("model")
                .and_then(|m| m.get("providerID"))
                .and_then(|v| v.as_str())
        })
        .map(String::from);

    Some(MessageData {
        role,
        agent,
        model_id,
        provider_id,
    })
}

/// Parse part.data JSON column.
pub fn parse_part_data(json: &str, time_created: i64) -> Option<PartData> {
    let value: Value = serde_json::from_str(json).ok()?;
    let part_type = value.get("type")?.as_str()?.to_string();
    let tool_name = value.get("tool").and_then(|v| v.as_str()).map(String::from);
    let tool_status = value
        .get("state")
        .and_then(|s| s.get("status"))
        .and_then(|v| v.as_str())
        .map(String::from);
    let tool_input_summary = value
        .get("state")
        .and_then(|s| s.get("input"))
        .and_then(|i| {
            // Try common input fields for a summary
            i.get("command")
                .or_else(|| i.get("filePath"))
                .or_else(|| i.get("pattern"))
                .and_then(|v| v.as_str())
        })
        .map(String::from);

    Some(PartData {
        part_type,
        tool_name,
        tool_status,
        tool_input_summary,
        time_created,
    })
}

/// Check if a session title indicates it's a subagent session.
pub fn is_subagent_title(title: &str) -> bool {
    title.contains("subagent)")
}

/// Extract the subagent type from a title like "Fix tests (@explore subagent)".
pub fn extract_subagent_type(title: &str) -> Option<&str> {
    let start = title.find("(@")? + 2;
    let end = title[start..].find(" subagent)")? + start;
    Some(&title[start..end])
}

/// An OpenCode session from the SQLite database.
#[derive(Debug, Clone)]
pub struct OpenCodeSession {
    pub id: String,
    pub title: Option<String>,
    pub directory: Option<PathBuf>,
    pub parent_id: Option<String>,
    pub time_created: i64,
    pub time_updated: i64,
    pub time_archived: Option<i64>,
    pub branch: Option<String>,
}

/// Query OpenCode's SQLite database for active sessions and their state.
pub fn query_opencode_sessions(db_path: &Path) -> anyhow::Result<Vec<OpenCodeSession>> {
    let conn = rusqlite::Connection::open_with_flags(
        db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    query_opencode_sessions_conn(&conn)
}

/// Query active sessions using an already-open connection.
///
/// Prefer this over [`query_opencode_sessions`] when the connection is shared
/// across multiple queries in the same poll cycle.
pub fn query_opencode_sessions_conn(
    conn: &rusqlite::Connection,
) -> anyhow::Result<Vec<OpenCodeSession>> {
    let mut stmt = conn.prepare(
        "SELECT s.id, s.title, s.directory, s.parent_id, s.time_created, s.time_updated, s.time_archived,
                w.branch
         FROM session s
         LEFT JOIN workspace w ON s.workspace_id = w.id
         WHERE s.time_archived IS NULL
         ORDER BY s.time_updated DESC
         LIMIT 50",
    )?;

    let sessions = stmt
        .query_map([], |row| {
            Ok(OpenCodeSession {
                id: row.get(0)?,
                title: row.get(1)?,
                directory: row.get::<_, Option<String>>(2)?.map(PathBuf::from),
                parent_id: row.get(3)?,
                time_created: row.get(4)?,
                time_updated: row.get(5)?,
                time_archived: row.get(6)?,
                branch: row.get(7)?,
            })
        })?
        .filter_map(|r| match r {
            Ok(s) => Some(s),
            Err(e) => {
                debug!("failed to parse session row: {e}");
                None
            }
        })
        .collect();

    Ok(sessions)
}

/// Query recent parts (tool calls, steps) for a session.
pub fn query_recent_parts(
    db_path: &Path,
    session_id: &str,
    limit: usize,
) -> anyhow::Result<Vec<PartData>> {
    let conn = rusqlite::Connection::open_with_flags(
        db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;

    let mut stmt = conn.prepare(
        "SELECT data, time_created FROM part WHERE session_id = ?1 ORDER BY time_created DESC LIMIT ?2",
    )?;

    let parts: Vec<PartData> = stmt
        .query_map(rusqlite::params![session_id, limit as i64], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?
        .filter_map(|r| match r {
            Ok((json, time_created)) => Some((json, time_created)),
            Err(e) => {
                debug!("failed to parse part row: {e}");
                None
            }
        })
        .filter_map(|(json, time_created)| parse_part_data(&json, time_created))
        .collect();

    Ok(parts)
}

/// Query subagent sessions for a parent session.
pub fn query_subagent_sessions(
    db_path: &Path,
    parent_session_id: &str,
) -> anyhow::Result<Vec<OpenCodeSession>> {
    let conn = rusqlite::Connection::open_with_flags(
        db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;

    let mut stmt = conn.prepare(
        "SELECT id, title, directory, parent_id, time_created, time_updated, time_archived, NULL as branch
         FROM session
         WHERE parent_id = ?1
         ORDER BY time_created DESC",
    )?;

    let sessions = stmt
        .query_map(rusqlite::params![parent_session_id], |row| {
            Ok(OpenCodeSession {
                id: row.get(0)?,
                title: row.get(1)?,
                directory: row.get::<_, Option<String>>(2)?.map(PathBuf::from),
                parent_id: row.get(3)?,
                time_created: row.get(4)?,
                time_updated: row.get(5)?,
                time_archived: row.get(6)?,
                branch: row.get(7)?,
            })
        })?
        .filter_map(|r| match r {
            Ok(s) => Some(s),
            Err(e) => {
                debug!("failed to parse subagent session row: {e}");
                None
            }
        })
        .collect();

    Ok(sessions)
}

/// Query todo items for a specific session from the OpenCode database.
pub fn query_session_todos(db_path: &Path, session_id: &str) -> anyhow::Result<Vec<TodoItem>> {
    let conn = rusqlite::Connection::open_with_flags(
        db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    let mut stmt = conn.prepare(
        "SELECT content, status, priority FROM todo WHERE session_id = ?1 ORDER BY position ASC",
    )?;
    let todos = stmt
        .query_map(rusqlite::params![session_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?
        .filter_map(|r| r.ok())
        .map(|(content, status, priority)| TodoItem {
            content,
            status: TodoStatus::parse_str(&status),
            priority: TodoPriority::parse_str(&priority),
        })
        .collect();
    Ok(todos)
}

/// Check whether the latest assistant message's turn is completed.
///
/// Returns `true` if `time.completed` is set on the latest assistant message,
/// indicating OpenCode's `cleanup()` has finished the turn.
pub fn check_assistant_turn_completed(
    conn: &rusqlite::Connection,
    session_id: &str,
) -> anyhow::Result<bool> {
    let mut stmt = conn.prepare(
        "SELECT json_extract(data, '$.time.completed') \
         FROM message \
         WHERE session_id = ?1 AND json_extract(data, '$.role') = 'assistant' \
         ORDER BY time_created DESC LIMIT 1",
    )?;

    let completed: Option<Option<i64>> = stmt
        .query_row(rusqlite::params![session_id], |row| {
            row.get::<_, Option<i64>>(0)
        })
        .optional()?;

    Ok(matches!(completed, Some(Some(_))))
}

/// Returns `true` if the latest assistant message in the session has a
/// `time.completed` timestamp set (meaning OpenCode's `cleanup()` ran and
/// the turn is finished). Returns `false` if the timestamp is NULL or if
/// there are no assistant messages.
pub fn query_assistant_turn_completed(db_path: &Path, session_id: &str) -> anyhow::Result<bool> {
    let conn = rusqlite::Connection::open_with_flags(
        db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    check_assistant_turn_completed(&conn, session_id)
}

fn extract_text_fragments(value: &Value, fragments: &mut Vec<String>) {
    match value {
        Value::String(text) => {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                fragments.push(trimmed.to_string());
            }
        }
        Value::Array(items) => {
            for item in items {
                extract_text_fragments(item, fragments);
            }
        }
        Value::Object(map) => {
            if let Some(text) = map.get("text").and_then(Value::as_str) {
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    fragments.push(trimmed.to_string());
                }
            }
            if let Some(content) = map.get("content") {
                extract_text_fragments(content, fragments);
            }
            if let Some(parts) = map.get("parts") {
                extract_text_fragments(parts, fragments);
            }
        }
        _ => {}
    }
}

fn extract_assistant_message_text(json: &str) -> Option<String> {
    let value: Value = serde_json::from_str(json).ok()?;
    let mut fragments = Vec::new();

    if let Some(content) = value.get("content") {
        extract_text_fragments(content, &mut fragments);
    } else if let Some(text) = value.get("text") {
        extract_text_fragments(text, &mut fragments);
    }

    (!fragments.is_empty()).then(|| fragments.join("\n"))
}

/// Query the latest assistant message text for a session.
pub fn query_latest_assistant_message_text_conn(
    conn: &rusqlite::Connection,
    session_id: &str,
) -> anyhow::Result<Option<String>> {
    let mut stmt = conn.prepare(
        "SELECT data
         FROM message
         WHERE session_id = ?1 AND json_extract(data, '$.role') = 'assistant'
         ORDER BY time_created DESC
         LIMIT 1",
    )?;

    let raw: Option<String> = stmt
        .query_row(rusqlite::params![session_id], |row| row.get(0))
        .optional()?;

    Ok(raw.as_deref().and_then(extract_assistant_message_text))
}

fn contains_review_section_marker(text: &str, marker: &str) -> bool {
    text.lines().map(str::trim_start).any(|line| {
        let line =
            line.trim_start_matches(|ch: char| matches!(ch, '#' | '>' | '-' | '*' | ' ' | '\t'));
        let lower = line.to_ascii_lowercase();

        lower == marker
            || lower.starts_with(&format!("{marker}:"))
            || lower.starts_with(&format!("{marker} -"))
            || lower.starts_with(&format!("{marker} "))
    })
}

fn looks_like_review_output(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return false;
    }

    let lower = trimmed.to_ascii_lowercase();
    if [
        "### issues",
        "#### critical",
        "merge-readiness assessment",
        "review assessment",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
    {
        return true;
    }

    ["requested changes", "review findings", "audit findings"]
        .iter()
        .any(|marker| contains_review_section_marker(trimmed, marker))
}

pub fn normalize_opencode_state(
    session_state: Option<&str>,
    _latest_tool_name: Option<&str>,
    parts: &[PartData],
    assistant_turn_completed: bool,
    latest_assistant_message: Option<&str>,
) -> NormalizedState {
    if latest_assistant_message.is_some_and(looks_like_review_output) {
        return NormalizedState::new(
            NormalizedStatus::NeedsInput,
            Some(NormalizedReason::Review),
        );
    }

    if session_state == Some("retry")
        || parts
            .iter()
            .any(|part| matches!(part.part_type.as_str(), "retry" | "retrying"))
    {
        return NormalizedState::new(
            NormalizedStatus::Working,
            Some(NormalizedReason::Retrying),
        );
    }

    if parts
        .iter()
        .any(|part| matches!(part.tool_status.as_deref(), Some("running" | "pending")))
    {
        return NormalizedState::new(
            NormalizedStatus::Working,
            Some(NormalizedReason::ToolCall),
        );
    }

    if matches!(session_state, Some("busy" | "working" | "running")) {
        return NormalizedState::new(NormalizedStatus::Working, None);
    }

    if assistant_turn_completed {
        return NormalizedState::new(
            NormalizedStatus::Done,
            Some(NormalizedReason::ReadyForNextTask),
        );
    }

    NormalizedState::new(NormalizedStatus::Unknown, None)
}

pub fn observed_session_state(status: PrimaryStatus) -> Option<&'static str> {
    match status {
        PrimaryStatus::Running => Some("busy"),
        PrimaryStatus::Waiting => Some("idle"),
        _ => None,
    }
}

/// State tracked per session to avoid emitting duplicate events.
#[derive(Debug, Clone, Default)]
pub struct SessionEmitState {
    pub last_status: Option<PrimaryStatus>,
    pub last_normalized_state: Option<NormalizedState>,
    pub last_tool_call: Option<String>,
    /// Tracks the time_created of the most recent part we saw, so we can
    /// detect new activity even when the tool name/status hasn't changed.
    pub last_part_time: i64,
    pub emitted_subagent_ids: HashSet<String>,
    pub completed_subagent_ids: HashSet<String>,
    pub session_info_sent: bool,
    pub last_todos: Vec<TodoItem>,
}

/// Resolve the primary status from recent parts and whether the assistant
/// turn has completed.
///
/// Decision priority:
/// 1. Any tool part with `running` or `pending` status → Running
/// 2. Latest part type determines status (most map to Running)
/// 3. `step-finish` + `assistant_turn_completed` → Waiting
/// 4. No parts + `assistant_turn_completed` → Waiting
/// 5. Otherwise → Running
pub fn resolve_status(parts: &[PartData], assistant_turn_completed: bool) -> PrimaryStatus {
    // Priority 1: any running/pending tool anywhere in the list
    if parts
        .iter()
        .any(|p| matches!(p.tool_status.as_deref(), Some("running" | "pending")))
    {
        return PrimaryStatus::Running;
    }

    // Priority 2: look at the latest part
    if let Some(latest) = parts.first() {
        return match latest.part_type.as_str() {
            "step-finish" => {
                if assistant_turn_completed {
                    PrimaryStatus::Waiting
                } else {
                    PrimaryStatus::Running
                }
            }
            // All other part types indicate active work.
            // Note: OpenCode does not emit "error" parts — errors are stored
            // in the assistant message's `data.error` field, not as part types.
            _ => PrimaryStatus::Running,
        };
    }

    // No parts at all
    if assistant_turn_completed {
        PrimaryStatus::Waiting
    } else {
        PrimaryStatus::Running
    }
}

/// Infer an Activity from recent parts.
///
/// Examines the most recent tool state first. If the newest tool part in the
/// recent window is active (`running` or `pending`), report `ToolCall`.
/// Otherwise, fall back to the latest part type so stale tool snapshots do not
/// override newer terminal states.
pub fn infer_activity_from_parts(parts: &[PartData]) -> Option<Activity> {
    // Parts are ordered newest-first. Use the newest tool state we can see,
    // rather than letting older tool snapshots override newer terminal states.
    if let Some(latest_tool) = parts
        .iter()
        .find(|p| matches!(p.part_type.as_str(), "tool" | "tool-invocation"))
    {
        if matches!(
            latest_tool.tool_status.as_deref(),
            Some("running" | "pending")
        ) {
            return Some(Activity::ToolCall);
        }
    }

    // Fall back to latest part type
    if let Some(latest) = parts.first() {
        return match latest.part_type.as_str() {
            "compaction" => Some(Activity::Compacting),
            "tool" | "tool-invocation" => {
                // Tool completed/error/pending — model is thinking about next step
                Some(Activity::Thinking)
            }
            "step-start" => Some(Activity::Thinking),
            "text" => Some(Activity::Generating),
            "step-finish" => Some(Activity::Idle),
            "reasoning" => Some(Activity::Thinking),
            _ => None,
        };
    }
    None
}

/// Query todo items using an already-open connection.
///
/// Prefer this over [`query_session_todos`] when multiple queries share the
/// same poll-cycle connection to avoid repeated open/close overhead.
pub fn query_session_todos_conn(
    conn: &rusqlite::Connection,
    session_id: &str,
) -> anyhow::Result<Vec<TodoItem>> {
    let mut stmt = conn.prepare(
        "SELECT content, status, priority FROM todo WHERE session_id = ?1 ORDER BY position ASC",
    )?;
    let todos = stmt
        .query_map(rusqlite::params![session_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?
        .filter_map(|r| r.ok())
        .map(|(content, status, priority)| TodoItem {
            content,
            status: TodoStatus::parse_str(&status),
            priority: TodoPriority::parse_str(&priority),
        })
        .collect();
    Ok(todos)
}

/// Query subagent sessions using an already-open connection.
///
/// Prefer this over [`query_subagent_sessions`] when multiple queries share
/// the same poll-cycle connection.
pub fn query_subagent_sessions_conn(
    conn: &rusqlite::Connection,
    parent_session_id: &str,
) -> anyhow::Result<Vec<OpenCodeSession>> {
    let mut stmt = conn.prepare(
        "SELECT id, title, directory, parent_id, time_created, time_updated, time_archived, NULL as branch
         FROM session
         WHERE parent_id = ?1
         ORDER BY time_created DESC",
    )?;

    let sessions = stmt
        .query_map(rusqlite::params![parent_session_id], |row| {
            Ok(OpenCodeSession {
                id: row.get(0)?,
                title: row.get(1)?,
                directory: row.get::<_, Option<String>>(2)?.map(PathBuf::from),
                parent_id: row.get(3)?,
                time_created: row.get(4)?,
                time_updated: row.get(5)?,
                time_archived: row.get(6)?,
                branch: row.get(7)?,
            })
        })?
        .filter_map(|r| match r {
            Ok(s) => Some(s),
            Err(e) => {
                debug!("failed to parse subagent session row: {e}");
                None
            }
        })
        .collect();

    Ok(sessions)
}

/// Collect [`AgentEvent`]s for a session's subagents and update `state`.
///
/// - Emits [`AgentEvent::SubagentSpawned`] the first time a subagent is seen.
/// - Emits [`AgentEvent::SubagentCompleted`] exactly once when an already-seen
///   subagent becomes archived.
pub fn collect_subagent_events(
    subagents: &[OpenCodeSession],
    state: &mut SessionEmitState,
    tool: crate::types::AgentTool,
    pid: u32,
) -> Vec<AgentEvent> {
    let mut events = Vec::new();

    for sub in subagents {
        if state.emitted_subagent_ids.contains(&sub.id) {
            // Already announced — check for first-time completion.
            if sub.time_archived.is_some()
                && !state.completed_subagent_ids.contains(&sub.id)
            {
                let name = sub
                    .title
                    .as_deref()
                    .and_then(extract_subagent_type)
                    .unwrap_or("unknown")
                    .to_string();
                events.push(AgentEvent::SubagentCompleted {
                    id: crate::types::AgentId::new(tool, pid),
                    pid: Some(pid),
                    subagent_name: name,
                });
                state.completed_subagent_ids.insert(sub.id.clone());
            }
            continue;
        }

        // First time we see this subagent — emit SubagentSpawned.
        if let Some(ref title) = sub.title {
            let subagent_type = extract_subagent_type(title)
                .unwrap_or("unknown")
                .to_string();
            events.push(AgentEvent::SubagentSpawned {
                id: crate::types::AgentId::new(tool, pid),
                pid: Some(pid),
                subagent: crate::types::Subagent {
                    name: subagent_type,
                    status: if sub.time_archived.is_some() {
                        crate::types::PrimaryStatus::Done
                    } else {
                        crate::types::PrimaryStatus::Running
                    },
                    description: sub.title.clone(),
                    started_at: crate::types::from_epoch_millis(sub.time_created)
                        .unwrap_or_else(chrono::Local::now),
                    ended_at: None,
                },
            });
            state.emitted_subagent_ids.insert(sub.id.clone());
            // If already archived when first discovered, mark completed immediately
            // to prevent a spurious SubagentCompleted event on the very next poll.
            if sub.time_archived.is_some() {
                state.completed_subagent_ids.insert(sub.id.clone());
            }
        }
    }

    events
}

/// Run the OpenCode detector loop.
pub async fn run_opencode_detector(
    tx: mpsc::Sender<AgentEvent>,
    opencode_data_dir: PathBuf,
    poll_interval: Duration,
    registry: AgentRegistry,
) -> anyhow::Result<()> {
    let db_path = opencode_data_dir.join("opencode.db");
    let mut session_states: HashMap<String, SessionEmitState> = HashMap::new();

    loop {
        if db_path.exists() {
            // Open ONE connection for the entire poll cycle and query all sessions
            // plus their per-session data (parts, todos, subagents) in a single
            // spawn_blocking call to avoid the repeated open/close overhead.
            let db2 = db_path.clone();
            let registry2 = registry.clone();
            type SessionData = (
                OpenCodeSession,
                u32,             // pid
                Vec<PartData>,   // recent parts
                bool,            // assistant_turn_completed
                Option<String>,  // latest assistant message text
                Vec<TodoItem>,   // todos
                Vec<OpenCodeSession>, // subagent sessions
            );
            let poll_result = tokio::task::spawn_blocking(move || -> anyhow::Result<Vec<SessionData>> {
                let conn = rusqlite::Connection::open_with_flags(
                    &db2,
                    rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY
                        | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
                )?;

                let sessions = query_opencode_sessions_conn(&conn)?;
                let mut results: Vec<SessionData> = Vec::new();

                for session in sessions {
                    // Skip subagent sessions — they are tracked via subagent queries below.
                    if session.parent_id.is_some() {
                        continue;
                    }
                    let cwd = match &session.directory {
                        Some(d) => d.clone(),
                        None => continue,
                    };
                    let pid = match registry2.find_pid(AgentTool::OpenCode, &cwd) {
                        Some(p) => p,
                        None => continue, // process not yet discovered
                    };

                    // Reuse the same connection for all per-session queries.
                    let mut stmt = conn.prepare_cached(
                        "SELECT data, time_created FROM part \
                         WHERE session_id = ?1 ORDER BY time_created DESC LIMIT ?2",
                    )?;
                    let parts: Vec<PartData> = stmt
                        .query_map(rusqlite::params![&session.id, 5_i64], |row| {
                            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
                        })?
                        .filter_map(|r| match r {
                            Ok(t) => Some(t),
                            Err(e) => {
                                debug!("failed to parse part row: {e}");
                                None
                            }
                        })
                        .filter_map(|(json, tc)| parse_part_data(&json, tc))
                        .collect();

                    let assistant_turn_completed =
                        check_assistant_turn_completed(&conn, &session.id)?;
                    let latest_assistant_message =
                        query_latest_assistant_message_text_conn(&conn, &session.id)?;
                    let todos = query_session_todos_conn(&conn, &session.id)?;
                    let subs = query_subagent_sessions_conn(&conn, &session.id)?;

                    results.push((
                        session,
                        pid,
                        parts,
                        assistant_turn_completed,
                        latest_assistant_message,
                        todos,
                        subs,
                    ));
                }
                Ok(results)
            })
            .await;

            match poll_result {
                Ok(Ok(session_data)) => {
                    // Build the set of active session IDs (owning, not borrowed from
                    // session_data) so we can consume session_data in the loop below.
                    let active_session_ids: HashSet<String> = session_data
                        .iter()
                        .map(|(s, ..)| s.id.clone())
                        .collect();

                    for (
                        session,
                        pid,
                        parts,
                        assistant_turn_completed,
                        latest_assistant_message,
                        todos,
                        subs,
                    ) in
                        session_data
                    {
                        let state = session_states.entry(session.id.clone()).or_default();

                        // Emit SessionInfo once per session
                        if !state.session_info_sent {
                            let session_started = from_epoch_millis(session.time_created);
                            let _ = tx
                                .send(AgentEvent::SessionInfo {
                                    id: AgentId::new(AgentTool::OpenCode, pid),
                                    pid: Some(pid),
                                    session_id: session.id.clone(),
                                    branch: session.branch.clone(),
                                    is_worktree: false,
                                    started_at: session_started,
                                })
                                .await;
                            state.session_info_sent = true;
                        }

                        // ── Parts / status / activity ──────────────────────────────
                        let newest_time =
                            parts.first().map(|p| p.time_created).unwrap_or(0);
                        let has_new_parts = newest_time > state.last_part_time;
                        if has_new_parts {
                            state.last_part_time = newest_time;
                        }

                        let status = resolve_status(&parts, assistant_turn_completed);
                        let latest_tool_name = parts
                            .iter()
                            .find(|part| {
                                matches!(part.part_type.as_str(), "tool" | "tool-invocation")
                            })
                            .and_then(|part| part.tool_name.as_deref());
                        let normalized_state = normalize_opencode_state(
                            observed_session_state(status),
                            latest_tool_name,
                            &parts,
                            assistant_turn_completed,
                            latest_assistant_message.as_deref(),
                        );

                        if state.last_status != Some(status) {
                            let old = state.last_status.unwrap_or(PrimaryStatus::Running);
                            let _ = tx
                                .send(AgentEvent::StatusChanged {
                                    id: AgentId::new(AgentTool::OpenCode, pid),
                                    pid: Some(pid),
                                    old,
                                    new: status,
                                })
                                .await;
                            state.last_status = Some(status);
                        }

                        if state.last_normalized_state.as_ref() != Some(&normalized_state) {
                            let _ = tx
                                .send(AgentEvent::NormalizedStateChanged {
                                    id: AgentId::new(AgentTool::OpenCode, pid),
                                    pid: Some(pid),
                                    state: normalized_state.clone(),
                                })
                                .await;
                            state.last_normalized_state = Some(normalized_state);
                        }

                        if has_new_parts {
                            let activity = infer_activity_from_parts(&parts);
                            let _ = tx
                                .send(AgentEvent::ActivityChanged {
                                    id: AgentId::new(AgentTool::OpenCode, pid),
                                    pid: Some(pid),
                                    activity,
                                })
                                .await;
                        }

                        if has_new_parts {
                            for part in &parts {
                                if part.part_type == "tool"
                                    || part.part_type == "tool-invocation"
                                {
                                    if let Some(ref tool_name) = part.tool_name {
                                        let key = format!(
                                            "{}:{}:{}",
                                            tool_name,
                                            part.tool_input_summary
                                                .as_deref()
                                                .unwrap_or(""),
                                            part.time_created,
                                        );
                                        if state.last_tool_call.as_deref() != Some(&key) {
                                            let _ = tx
                                                .send(AgentEvent::ToolCallStarted {
                                                    id: AgentId::new(
                                                        AgentTool::OpenCode,
                                                        pid,
                                                    ),
                                                    pid: Some(pid),
                                                    tool_name: tool_name.clone(),
                                                    args_summary: part
                                                        .tool_input_summary
                                                        .clone(),
                                                })
                                                .await;
                                            state.last_tool_call = Some(key);
                                        }
                                        break;
                                    }
                                }
                            }
                        }

                        // ── Subagents ──────────────────────────────────────────────
                        for event in
                            collect_subagent_events(&subs, state, AgentTool::OpenCode, pid)
                        {
                            let _ = tx.send(event).await;
                        }

                        // ── Todos ──────────────────────────────────────────────────
                        if todos != state.last_todos {
                            let _ = tx
                                .send(AgentEvent::TodosUpdated {
                                    id: AgentId::new(AgentTool::OpenCode, pid),
                                    pid: Some(pid),
                                    todos: todos.clone(),
                                })
                                .await;
                            state.last_todos = todos; // move; clone went into event
                        }
                    }

                    // Prune state for sessions that are no longer active (archived or gone).
                    // This prevents the session_states map from growing without bound.
                    session_states.retain(|id, _| active_session_ids.contains(id.as_str()));
                }
                Ok(Err(e)) => {
                    debug!("Failed to query OpenCode DB: {e}");
                }
                Err(e) => {
                    debug!("spawn_blocking panicked for OpenCode poll: {e}");
                }
            }
        }

        tokio::time::sleep(poll_interval).await;
    }
}
