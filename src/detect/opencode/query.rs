use std::path::Path;

use rusqlite::OptionalExtension;
use serde_json::Value;
use tracing::debug;

use crate::domain::agents::activity::{TodoItem, TodoPriority, TodoStatus};

use super::types::{AssistantMessageData, MessageData, OpenCodeSession, PartData};

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
                directory: row
                    .get::<_, Option<String>>(2)?
                    .map(std::path::PathBuf::from),
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
                directory: row
                    .get::<_, Option<String>>(2)?
                    .map(std::path::PathBuf::from),
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

pub(super) fn extract_text_fragments(value: &Value, fragments: &mut Vec<String>) {
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
            if let Some(message) = map.get("message") {
                extract_text_fragments(message, fragments);
            }
        }
        _ => {}
    }
}

pub(super) fn extract_assistant_message_data(json: &str) -> Option<AssistantMessageData> {
    let value: Value = serde_json::from_str(json).ok()?;
    let mut text_fragments = Vec::new();
    let mut error_fragments = Vec::new();

    if let Some(content) = value.get("content") {
        extract_text_fragments(content, &mut text_fragments);
    } else if let Some(text) = value.get("text") {
        extract_text_fragments(text, &mut text_fragments);
    }

    if let Some(error) = value.get("error") {
        extract_text_fragments(error, &mut error_fragments);
    }

    Some(AssistantMessageData {
        text: (!text_fragments.is_empty()).then(|| text_fragments.join("\n")),
        error: (!error_fragments.is_empty()).then(|| error_fragments.join("\n")),
    })
}

/// Query the latest assistant message text for a session.
pub fn query_latest_assistant_message_text_conn(
    conn: &rusqlite::Connection,
    session_id: &str,
) -> anyhow::Result<Option<String>> {
    Ok(query_latest_assistant_message_data_conn(conn, session_id)?.and_then(|data| data.text))
}

/// Query the latest assistant message data for a session.
pub fn query_latest_assistant_message_data_conn(
    conn: &rusqlite::Connection,
    session_id: &str,
) -> anyhow::Result<Option<AssistantMessageData>> {
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

    Ok(raw.as_deref().and_then(extract_assistant_message_data))
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
                directory: row
                    .get::<_, Option<String>>(2)?
                    .map(std::path::PathBuf::from),
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
