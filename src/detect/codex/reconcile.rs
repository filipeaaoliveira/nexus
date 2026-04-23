use std::collections::HashMap;

use chrono::Utc;

use crate::event::{AgentEvent, AgentRef};
use nexus_domain::agents::activity::{Activity, Subagent};
use nexus_domain::agents::ids::{AgentId, AgentTool};
use nexus_domain::agents::status::PrimaryStatus;

use super::normalization::{infer_codex_activity, normalize_codex_session};
use super::types::{CodexEventSnapshot, ObservedCodexSession, ObservedCodexSubagent};

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
                agent: AgentRef::new(session.agent_id.clone(), session.pid),
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
                agent: AgentRef::new(session.agent_id.clone(), session.pid),
                session_id: Some(session_id.to_string()),
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
                agent: AgentRef::new(session.agent_id.clone(), session.pid),
                old: previous_status.unwrap_or(PrimaryStatus::Running),
                new: session.status,
            });
        }

        let normalized_state = normalize_codex_session(session, session.final_output.as_deref());
        let previous_normalized_state = previous_session.map(|previous_session| {
            normalize_codex_session(previous_session, previous_session.final_output.as_deref())
        });
        if previous_normalized_state.as_ref() != Some(&normalized_state) {
            events.push(AgentEvent::NormalizedStateChanged {
                agent: AgentRef::new(session.agent_id.clone(), session.pid),
                state: normalized_state,
            });
        }

        let previous_todos = previous_session.map(|session| &session.todos);
        if previous_todos != Some(&session.todos) {
            events.push(AgentEvent::TodosUpdated {
                agent: AgentRef::new(session.agent_id.clone(), session.pid),
                todos: session.todos.clone(),
            });
        }

        let current_activity = infer_codex_activity(session);
        let previous_activity = previous_session.map(infer_codex_activity);
        // Only emit when the activity actually changed.  For newly discovered
        // sessions (previous_session = None) suppress the event when the
        // session is already idle — there is no meaningful transition to
        // announce and it avoids a spurious event flood on startup.
        let activity_changed = match previous_activity {
            Some(ref prev) => prev != &current_activity,
            None => current_activity != Some(Activity::Idle),
        };
        if activity_changed {
            events.push(AgentEvent::ActivityChanged {
                agent: AgentRef::new(session.agent_id.clone(), session.pid),
                activity: current_activity,
            });
        }

        let previous_tool_call =
            previous_session.and_then(|session| session.latest_tool_call.as_ref());
        match (previous_tool_call, session.latest_tool_call.as_ref()) {
            (Some(previous_tool_call), Some(current_tool_call))
                if previous_tool_call != current_tool_call =>
            {
                events.push(AgentEvent::ToolCallCompleted {
                    agent: AgentRef::new(session.agent_id.clone(), session.pid),
                });
                events.push(AgentEvent::ToolCallStarted {
                    agent: AgentRef::new(session.agent_id.clone(), session.pid),
                    tool_name: current_tool_call.name.clone(),
                    args_summary: current_tool_call.args_summary.clone(),
                });
            }
            (Some(_previous_tool_call), None) => {
                events.push(AgentEvent::ToolCallCompleted {
                    agent: AgentRef::new(session.agent_id.clone(), session.pid),
                });
            }
            (None, Some(current_tool_call)) => {
                events.push(AgentEvent::ToolCallStarted {
                    agent: AgentRef::new(session.agent_id.clone(), session.pid),
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

        let current_subagents = session
            .subagents
            .iter()
            .map(|subagent| Subagent {
                name: subagent.name.clone(),
                status: subagent.status,
                reason: subagent.reason.clone(),
                description: subagent.description.clone(),
                started_at: Utc::now(),
                ended_at: (subagent.status == PrimaryStatus::Done).then(Utc::now),
            })
            .collect::<Vec<_>>();

        let previous_subagents_vec = previous_session
            .map(|session| {
                session
                    .subagents
                    .iter()
                    .map(|subagent| {
                        (
                            subagent.thread_id.as_str(),
                            &subagent.name,
                            subagent.status,
                            subagent.reason.as_ref(),
                            subagent.description.as_ref(),
                        )
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let current_subagents_vec = session
            .subagents
            .iter()
            .map(|subagent| {
                (
                    subagent.thread_id.as_str(),
                    &subagent.name,
                    subagent.status,
                    subagent.reason.as_ref(),
                    subagent.description.as_ref(),
                )
            })
            .collect::<Vec<_>>();
        if previous_subagents_vec != current_subagents_vec {
            events.push(AgentEvent::SubagentsUpdated {
                agent: AgentRef::new(session.agent_id.clone(), session.pid),
                subagents: current_subagents,
            });
        }

        for subagent in &session.subagents {
            match previous_subagents.get(subagent.thread_id.as_str()).copied() {
                None => {
                    events.push(AgentEvent::SubagentSpawned {
                        agent: AgentRef::new(session.agent_id.clone(), session.pid),
                        subagent: Subagent {
                            name: subagent.name.clone(),
                            status: subagent.status,
                            reason: subagent.reason.clone(),
                            description: subagent.description.clone(),
                            started_at: subagent.started_at.unwrap_or_else(Utc::now),
                            ended_at: (subagent.status == PrimaryStatus::Done).then(Utc::now),
                        },
                    });
                }
                Some(previous_subagent)
                    if previous_subagent.status != PrimaryStatus::Done
                        && subagent.status == PrimaryStatus::Done =>
                {
                    events.push(AgentEvent::SubagentCompleted {
                        agent: AgentRef::new(session.agent_id.clone(), session.pid),
                        subagent_name: subagent.thread_id.clone(),
                    });
                }
                _ => {}
            }
        }
    }

    if previous_sessions.contains_key("codex-backend")
        && !current
            .sessions
            .iter()
            .any(|session| session.agent_id.session_id() == Some("codex-backend"))
    {
        events.push(AgentEvent::ProcessExited {
            agent: AgentRef::new(
                AgentId::session(AgentTool::Codex, "codex-backend"),
                Some(0),
            ),
        });
    }

    events
}
