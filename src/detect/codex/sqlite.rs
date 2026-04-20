use std::path::Path;

use tracing::debug;

use crate::detect::error::DetectorError;
use super::types::{CodexThread, SourceInfo, SpawnEdge};

// ── Source Field Parsing ──────────────────────────────────────────

/// Parse the `source` field from a Codex session meta event.
///
/// If the source starts with `{`, it's parsed as JSON to extract subagent
/// spawn information. Otherwise, it's treated as a simple string (e.g., "cli").
pub fn parse_source_field(source: &str) -> SourceInfo {
    if source.starts_with('{') {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(source) {
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
pub fn query_codex_threads(db_path: &Path) -> Result<Vec<CodexThread>, DetectorError> {
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

/// Query all spawn edges (parent→child thread relationships) from the SQLite database.
pub fn query_all_spawn_edges(db_path: &Path) -> Result<Vec<SpawnEdge>, DetectorError> {
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
