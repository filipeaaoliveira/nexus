use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::OptionalExtension;
use serde_json::Value;
use tokio::sync::mpsc;
use tracing::debug;

use crate::detect::registry::AgentRegistry;
use crate::event::AgentEvent;
use crate::types::{Activity, AgentTool, PrimaryStatus};

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

/// Query the role of the latest message in a session.
///
/// Returns `Some("assistant")` or `Some("user")` for the most recent message,
/// or `None` if no messages exist for the given session.
pub fn query_latest_message_role(
    db_path: &Path,
    session_id: &str,
) -> anyhow::Result<Option<String>> {
    let conn = rusqlite::Connection::open_with_flags(
        db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;

    let mut stmt = conn.prepare(
        "SELECT json_extract(data, '$.role') FROM message WHERE session_id = ?1 ORDER BY time_created DESC LIMIT 1",
    )?;

    let role = stmt
        .query_row(rusqlite::params![session_id], |row| row.get::<_, Option<String>>(0))
        .optional()?
        .flatten();

    Ok(role)
}

/// State tracked per session to avoid emitting duplicate events.
#[derive(Debug, Clone, Default)]
struct SessionEmitState {
    last_status: Option<PrimaryStatus>,
    last_tool_call: Option<String>,
    /// Tracks the time_created of the most recent part we saw, so we can
    /// detect new activity even when the tool name/status hasn't changed.
    last_part_time: i64,
    emitted_subagent_ids: Vec<String>,
    session_info_sent: bool,
}

/// Infer a primary status from recent parts for an OpenCode session.
///
/// Every part type maps to `Running` except `"error"` → `Error`.
/// The explicit match documents each known part type for clarity.
pub fn infer_status_from_parts(parts: &[PartData]) -> PrimaryStatus {
    // Parts are ordered most-recent-first from the query
    if let Some(latest) = parts.first() {
        if latest.part_type == "error" {
            return PrimaryStatus::Error;
        }
    }
    PrimaryStatus::Running
}

/// Resolve the final status from the inferred status, the latest message role,
/// and the parts.
///
/// - If `inferred` is `Error`, return `Error`.
/// - If `latest_role` is `"assistant"` and no parts have `tool_status == "running"`,
///   the agent is waiting for user input → return `Waiting`.
/// - Otherwise return `inferred`.
pub fn resolve_status(
    inferred: PrimaryStatus,
    latest_role: Option<&str>,
    parts: &[PartData],
) -> PrimaryStatus {
    if inferred == PrimaryStatus::Error {
        return PrimaryStatus::Error;
    }
    if latest_role == Some("assistant")
        && !parts
            .iter()
            .any(|p| p.tool_status.as_deref() == Some("running"))
    {
        return PrimaryStatus::Waiting;
    }
    inferred
}

/// Infer an Activity from recent parts.
fn infer_activity_from_parts(parts: &[PartData]) -> Option<Activity> {
    if let Some(latest) = parts.first() {
        match latest.part_type.as_str() {
            "tool" | "tool-invocation" => {
                if latest.tool_status.as_deref() == Some("running") {
                    return Some(Activity::ToolCall);
                }
                // Tool completed, model is thinking about next step
                return Some(Activity::Thinking);
            }
            "step-start" => return Some(Activity::Thinking),
            "text" => return Some(Activity::Generating),
            "step-finish" => return Some(Activity::Idle),
            _ => {}
        }
    }
    None
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
            let db = db_path.clone();
            let result = tokio::task::spawn_blocking(move || {
                query_opencode_sessions(&db)
            })
            .await?;

            match result {
                Ok(sessions) => {
                    for session in &sessions {
                        // Skip subagent sessions — they share the parent's CWD
                        // and would incorrectly emit events against the parent PID.
                        // Subagents are tracked via query_subagent_sessions below.
                        if session.parent_id.is_some() {
                            continue;
                        }

                        let cwd = match &session.directory {
                            Some(d) => d,
                            None => continue,
                        };

                        let pid = match registry.find_pid(AgentTool::OpenCode, cwd) {
                            Some(p) => p,
                            None => continue, // no matching process discovered yet
                        };

                        let state = session_states
                            .entry(session.id.clone())
                            .or_default();

                        // Emit SessionInfo once per session
                        if !state.session_info_sent {
                            let _ = tx
                                .send(AgentEvent::SessionInfo {
                                    pid,
                                    tool: AgentTool::OpenCode,
                                    session_id: session.id.clone(),
                                    branch: session.branch.clone(),
                                    is_worktree: false,
                                })
                                .await;
                            state.session_info_sent = true;
                        }

                        // Query recent parts and latest message role in a single blocking call
                        let session_id = session.id.clone();
                        let db2 = db_path.clone();
                        let db_result = tokio::task::spawn_blocking(move || {
                            let conn = rusqlite::Connection::open_with_flags(
                                &db2,
                                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY
                                    | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
                            )?;

                            // Query recent parts
                            let mut stmt = conn.prepare(
                                "SELECT data, time_created FROM part WHERE session_id = ?1 ORDER BY time_created DESC LIMIT ?2",
                            )?;
                            let parts: Vec<PartData> = stmt
                                .query_map(rusqlite::params![&session_id, 5_i64], |row| {
                                    Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
                                })?
                                .filter_map(|r| r.ok())
                                .filter_map(|(json, tc)| parse_part_data(&json, tc))
                                .collect();

                            // Query latest message role
                            let mut stmt2 = conn.prepare(
                                "SELECT json_extract(data, '$.role') FROM message WHERE session_id = ?1 ORDER BY time_created DESC LIMIT 1",
                            )?;
                            let role: Option<String> = stmt2
                                .query_row(rusqlite::params![&session_id], |row| {
                                    row.get::<_, Option<String>>(0)
                                })
                                .optional()?
                                .flatten();

                            Ok::<_, anyhow::Error>((parts, role))
                        })
                        .await;

                        if let Ok(Ok((parts, latest_role))) = db_result {
                            // Check if there's new activity by comparing the most
                            // recent part's time_created against what we saw last poll.
                            let newest_time = parts.first().map(|p| p.time_created).unwrap_or(0);
                            let has_new_parts = newest_time > state.last_part_time;
                            if has_new_parts {
                                state.last_part_time = newest_time;
                            }

                            // Determine final status using resolve_status.
                            let inferred = infer_status_from_parts(&parts);
                            let status = resolve_status(
                                inferred,
                                latest_role.as_deref(),
                                &parts,
                            );

                            // Emit status change if different from last known
                            if state.last_status != Some(status) {
                                let old = state.last_status.unwrap_or(PrimaryStatus::Running);
                                let _ = tx
                                    .send(AgentEvent::StatusChanged {
                                        pid,
                                        tool: AgentTool::OpenCode,
                                        old,
                                        new: status,
                                    })
                                    .await;
                                state.last_status = Some(status);
                            }

                            // Emit activity change on every poll that sees new parts
                            if has_new_parts {
                                let activity = infer_activity_from_parts(&parts);
                                let _ = tx
                                    .send(AgentEvent::ActivityChanged {
                                        pid,
                                        tool: AgentTool::OpenCode,
                                        activity,
                                    })
                                    .await;
                            }

                            // Emit tool call for the most recent tool part if new
                            if has_new_parts {
                                for part in &parts {
                                    if part.part_type == "tool" || part.part_type == "tool-invocation" {
                                        if let Some(ref tool_name) = part.tool_name {
                                            let key = format!(
                                                "{}:{}:{}",
                                                tool_name,
                                                part.tool_input_summary.as_deref().unwrap_or(""),
                                                part.time_created,
                                            );
                                            if state.last_tool_call.as_deref() != Some(&key) {
                                                let _ = tx
                                                    .send(AgentEvent::ToolCallStarted {
                                                        pid,
                                                        tool: AgentTool::OpenCode,
                                                        tool_name: tool_name.clone(),
                                                        args_summary: part
                                                            .tool_input_summary
                                                            .clone(),
                                                    })
                                                    .await;
                                                state.last_tool_call = Some(key);
                                            }
                                            break; // only emit for the most recent tool call
                                        }
                                    }
                                }
                            }
                        }

                        // Query subagent sessions
                        let parent_id = session.id.clone();
                        let db3 = db_path.clone();
                        if let Ok(Ok(subs)) = tokio::task::spawn_blocking(move || {
                            query_subagent_sessions(&db3, &parent_id)
                        })
                        .await
                        {
                            for sub in &subs {
                                // Skip already-emitted subagents
                                if state.emitted_subagent_ids.contains(&sub.id) {
                                    // Check if subagent completed
                                    if sub.time_archived.is_some() {
                                        let name = sub
                                            .title
                                            .as_deref()
                                            .and_then(extract_subagent_type)
                                            .unwrap_or("unknown")
                                            .to_string();
                                        let _ = tx
                                            .send(AgentEvent::SubagentCompleted {
                                                pid,
                                                tool: AgentTool::OpenCode,
                                                subagent_name: name,
                                            })
                                            .await;
                                    }
                                    continue;
                                }

                                if let Some(ref title) = sub.title {
                                    let subagent_type = extract_subagent_type(title)
                                        .unwrap_or("unknown")
                                        .to_string();
                                    let _ = tx
                                        .send(AgentEvent::SubagentSpawned {
                                            pid,
                                            tool: AgentTool::OpenCode,
                                            subagent: crate::types::Subagent {
                                                name: subagent_type,
                                                status: if sub.time_archived.is_some() {
                                                    PrimaryStatus::Done
                                                } else {
                                                    PrimaryStatus::Running
                                                },
                                                description: sub.title.clone(),
                                                started_at: chrono::Local::now(),
                                                ended_at: None,
                                            },
                                        })
                                        .await;
                                    state.emitted_subagent_ids.push(sub.id.clone());
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    debug!("Failed to query OpenCode DB: {e}");
                }
            }
        }

        tokio::time::sleep(poll_interval).await;
    }
}
