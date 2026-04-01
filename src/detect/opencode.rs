use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::Value;
use tokio::sync::mpsc;
use tracing::debug;

use crate::detect::registry::AgentRegistry;
use crate::event::AgentEvent;
use crate::types::{AgentTool, PrimaryStatus};

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
pub fn parse_part_data(json: &str) -> Option<PartData> {
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
        "SELECT data FROM part WHERE session_id = ?1 ORDER BY time_created DESC LIMIT ?2",
    )?;

    let parts: Vec<PartData> = stmt
        .query_map(rusqlite::params![session_id, limit as i64], |row| {
            row.get::<_, String>(0)
        })?
        .filter_map(|r| match r {
            Ok(json) => Some(json),
            Err(e) => {
                debug!("failed to parse part row: {e}");
                None
            }
        })
        .filter_map(|json| parse_part_data(&json))
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

/// State tracked per session to avoid emitting duplicate events.
#[derive(Debug, Clone, Default)]
struct SessionEmitState {
    last_status: Option<PrimaryStatus>,
    last_tool_call: Option<String>,
    emitted_subagent_ids: Vec<String>,
    session_info_sent: bool,
}

/// Infer a primary status from recent parts for an OpenCode session.
fn infer_status_from_parts(parts: &[PartData]) -> PrimaryStatus {
    // Parts are ordered most-recent-first from the query
    if let Some(latest) = parts.first() {
        match latest.part_type.as_str() {
            "tool" | "tool-invocation" => {
                if latest.tool_status.as_deref() == Some("running") {
                    return PrimaryStatus::Running;
                }
                return PrimaryStatus::Running;
            }
            "step-finish" => return PrimaryStatus::Running,
            "text" => return PrimaryStatus::Running,
            "error" => return PrimaryStatus::Error,
            _ => {}
        }
    }
    PrimaryStatus::Running
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

                        // Query recent parts for tool call and status events
                        let session_id = session.id.clone();
                        let db2 = db_path.clone();
                        if let Ok(Ok(parts)) = tokio::task::spawn_blocking(move || {
                            query_recent_parts(&db2, &session_id, 5)
                        })
                        .await
                        {
                            // Emit status change if different from last known
                            let inferred = infer_status_from_parts(&parts);
                            if state.last_status != Some(inferred) {
                                let old = state.last_status.unwrap_or(PrimaryStatus::Running);
                                let _ = tx
                                    .send(AgentEvent::StatusChanged {
                                        pid,
                                        tool: AgentTool::OpenCode,
                                        old,
                                        new: inferred,
                                    })
                                    .await;
                                state.last_status = Some(inferred);
                            }

                            // Emit tool call for the most recent tool part
                            for part in &parts {
                                if part.part_type == "tool" || part.part_type == "tool-invocation" {
                                    if let Some(ref tool_name) = part.tool_name {
                                        // Only emit if this is a different tool call
                                        let key = format!(
                                            "{}:{}",
                                            tool_name,
                                            part.tool_input_summary.as_deref().unwrap_or("")
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
