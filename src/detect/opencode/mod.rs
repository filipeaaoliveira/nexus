use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::Duration;

use tokio::sync::mpsc;
use tracing::debug;

use crate::detect::registry::RegistryHandle;
use crate::event::AgentEvent;
use nexus_domain::agents::activity::TodoItem;
use nexus_domain::agents::ids::{AgentId, AgentTool};
use nexus_domain::agents::status::PrimaryStatus;

mod inference;
mod query;
#[cfg(test)]
mod tests;
mod types;

pub use inference::*;
pub use query::*;
pub use types::*;

/// Run the OpenCode detector loop.
pub async fn run_opencode_detector(
    tx: mpsc::Sender<AgentEvent>,
    opencode_data_dir: PathBuf,
    poll_interval: Duration,
    registry: RegistryHandle,
) -> anyhow::Result<()> {
    let db_path = opencode_data_dir.join("opencode.db");
    tracing::info!("OpenCode detector starting, db_path={}", db_path.display());
    tracing::info!("OpenCode detector: db exists={}", db_path.exists());
    let mut session_states: HashMap<String, SessionEmitState> = HashMap::new();

    loop {
        if db_path.exists() {
            tracing::debug!("OpenCode detector: querying database");
            // Open ONE connection for the entire poll cycle and query all sessions
            // plus their per-session data (parts, todos, subagents) in a single
            // spawn_blocking call to avoid the repeated open/close overhead.
            let db2 = db_path.clone();
            let registry_snap = registry.snapshot().await;
            // Sessions where session_info_sent=true are already initialised; new ones
            // need a one-shot query for the most-recent tool part to seed recent_events.
            let known_sessions: HashSet<String> = session_states
                .iter()
                .filter(|(_, s)| s.session_info_sent)
                .map(|(id, _)| id.clone())
                .collect();
            type SessionData = (
                OpenCodeSession,
                u32,                                     // pid
                Vec<PartData>,                           // recent parts
                bool,                                    // assistant_turn_completed
                Option<AssistantMessageData>,            // latest assistant message data
                Vec<TodoItem>,                           // todos
                Vec<OpenCodeSession>,                    // subagent sessions
                Option<chrono::DateTime<chrono::Local>>, // process_started_at
                Option<PartData>,                        // initial tool call (new sessions only)
            );
            type PollData = (HashSet<String>, Vec<SessionData>);
            let poll_result = tokio::task::spawn_blocking(move || -> anyhow::Result<PollData> {
                let conn = rusqlite::Connection::open_with_flags(
                    &db2,
                    rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY
                        | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
                )?;

                let sessions = query_opencode_sessions_conn(&conn)?;
                let active_session_ids: HashSet<String> = sessions
                    .iter()
                    .filter(|session| session.parent_id.is_none())
                    .map(|session| session.id.clone())
                    .collect();
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
                    tracing::debug!(
                        "OpenCode: checking session {} dir={}",
                        session.id,
                        cwd.display()
                    );
                    let pid = match registry_snap.find_pid_prefix(AgentTool::OpenCode, &cwd) {
                        Some(p) => p,
                        None => {
                            tracing::debug!("OpenCode: no pid found for cwd={}", cwd.display());
                            continue; // process not yet discovered
                        }
                    };
                    let process_started_at = registry_snap
                        .find_opencode_launch(pid)
                        .and_then(|info| info.started_at);

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
                    let latest_assistant_message_data =
                        query_latest_assistant_message_data_conn(&conn, &session.id)?;
                    let todos = query_session_todos_conn(&conn, &session.id)?;
                    let subs = query_subagent_sessions_conn(&conn, &session.id)?;

                    // For new (not-yet-initialised) sessions, fetch the most recent
                    // *running or pending* tool/tool-invocation part so we can seed
                    // recent_events even when the last active tool call is beyond the
                    // LIMIT 5 recent-parts window.
                    //
                    // We restrict to running/pending because ToolCallStarted also
                    // sets agent.activity = ToolCall in the app; seeding from a
                    // completed tool would flash a misleading "tool call" activity
                    // state until the next ActivityChanged poll corrects it.
                    // Sessions with no in-progress tools are left with empty
                    // recent_events on first detection, which is accurate.
                    let initial_tool_call: Option<PartData> =
                        if !known_sessions.contains(&session.id) {
                            let mut tool_stmt = conn.prepare_cached(
                                "SELECT data, time_created FROM part \
                                 WHERE session_id = ?1 \
                                   AND (json_extract(data, '$.type') = 'tool' \
                                        OR json_extract(data, '$.type') = 'tool-invocation') \
                                   AND (json_extract(data, '$.state.status') = 'running' \
                                        OR json_extract(data, '$.state.status') = 'pending') \
                                 ORDER BY time_created DESC LIMIT 1",
                            )?;
                            let rows: Vec<(String, i64)> = tool_stmt
                                .query_map(rusqlite::params![&session.id], |row| {
                                    Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
                                })?
                                .filter_map(|r| r.ok())
                                .collect();
                            rows.into_iter()
                                .find_map(|(json, tc)| parse_part_data(&json, tc))
                        } else {
                            None
                        };

                    results.push((
                        session,
                        pid,
                        parts,
                        assistant_turn_completed,
                        latest_assistant_message_data,
                        todos,
                        subs,
                        process_started_at,
                        initial_tool_call,
                    ));
                }
                Ok((active_session_ids, results))
            })
            .await;

            match poll_result {
                Ok(Ok((active_session_ids, session_data))) => {
                    for (
                        session,
                        pid,
                        parts,
                        assistant_turn_completed,
                        latest_assistant_message_data,
                        todos,
                        subs,
                        process_started_at,
                        initial_tool_call,
                    ) in session_data
                    {
                        let state = session_states.entry(session.id.clone()).or_default();

                        // Emit SessionInfo once per session
                        if !state.session_info_sent {
                            let _ = tx
                                .send(AgentEvent::SessionInfo {
                                    id: AgentId::new(AgentTool::OpenCode, pid),
                                    pid: Some(pid),
                                    session_id: Some(session.id.clone()),
                                    branch: session.branch.clone(),
                                    is_worktree: false,
                                    started_at: None,
                                })
                                .await;
                            state.session_info_sent = true;

                            // Seed recent_events with the last known tool call so the
                            // detail panel doesn't show "no events yet" for sessions
                            // whose most recent tool call is beyond the LIMIT 5 window.
                            if let Some(ref part) = initial_tool_call {
                                if let Some(ref tool_name) = part.tool_name {
                                    let key = format!(
                                        "{}:{}:{}",
                                        tool_name,
                                        part.tool_input_summary.as_deref().unwrap_or(""),
                                        part.time_created,
                                    );
                                    let _ = tx
                                        .send(AgentEvent::ToolCallStarted {
                                            id: AgentId::new(AgentTool::OpenCode, pid),
                                            pid: Some(pid),
                                            tool_name: tool_name.clone(),
                                            args_summary: part.tool_input_summary.clone(),
                                        })
                                        .await;
                                    state.last_tool_call = Some(key);
                                }
                            }
                        }

                        // ── Parts / status / activity ──────────────────────────────
                        let newest_time = parts.first().map(|p| p.time_created).unwrap_or(0);
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
                            latest_assistant_message_data
                                .as_ref()
                                .and_then(|message| message.text.as_deref()),
                            latest_assistant_message_data
                                .as_ref()
                                .and_then(|message| message.error.as_deref()),
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
                                                    id: AgentId::new(AgentTool::OpenCode, pid),
                                                    pid: Some(pid),
                                                    tool_name: tool_name.clone(),
                                                    args_summary: part.tool_input_summary.clone(),
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
                        for event in collect_subagent_events(
                            &subs,
                            state,
                            AgentTool::OpenCode,
                            pid,
                            process_started_at,
                        ) {
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
