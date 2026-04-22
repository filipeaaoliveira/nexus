use std::collections::VecDeque;
use std::path::Path;

use chrono::{DateTime, Local};

use crate::app::agent::{
    ActivityEventKind, Agent, AgentDetail, AgentIdent, AgentLifecycle,
};
use crate::app::state::{OverviewState, SystemState};
use super::agent_lookup;
use crate::event::{AgentEvent, AgentRef};
use nexus_domain::agents::activity::Subagent;
use nexus_domain::agents::ids::{AgentId, AgentTool};
use nexus_domain::agents::status::{NormalizedState, NormalizedStatus, PrimaryStatus};

/// Pure reducer entry point for all agent events.
///
/// Takes explicit parameters — no `self`, no I/O, deterministic.
pub fn reduce(
    agents: &mut Vec<Agent>,
    overview: &mut OverviewState,
    system: &mut SystemState,
    event: &AgentEvent,
    now: DateTime<Local>,
) {
    match event {
        AgentEvent::DetectorHealthChanged {
            detector,
            healthy,
            message,
        } => {
            system.record_detector_health(*detector, *healthy, message);
        }
        AgentEvent::Discovered {
            agent,
            cwd,
            started_at,
        } => {
            reduce_discovered(agents, overview, &agent.id, agent.pid, cwd, *started_at, now);
        }
        AgentEvent::StatusChanged { agent, old, new } => {
            reduce_status_changed(agents, &agent.id, agent.pid, *old, *new, now);
        }
        AgentEvent::ActivityChanged { agent, activity } => {
            reduce_activity_changed(agents, &agent.id, agent.pid, *activity, now);
        }
        AgentEvent::NormalizedStateChanged { agent, state } => {
            reduce_normalized_state_changed(agents, &agent.id, agent.pid, state.clone(), now);
        }
        AgentEvent::SubagentSpawned { agent, subagent } => {
            reduce_subagent_spawned(agents, &agent.id, agent.pid, subagent, now);
        }
        AgentEvent::SubagentCompleted {
            agent,
            subagent_name,
        } => {
            reduce_subagent_completed(agents, &agent.id, agent.pid, subagent_name, now);
        }
        AgentEvent::SubagentsUpdated { agent, subagents } => {
            reduce_subagents_updated(agents, &agent.id, agent.pid, subagents, now);
        }
        AgentEvent::ToolCallStarted {
            agent,
            tool_name,
            args_summary,
        } => {
            reduce_tool_call_started(agents, &agent.id, agent.pid, tool_name, args_summary.as_deref(), now);
        }
        AgentEvent::ToolCallCompleted { agent } => {
            reduce_tool_call_completed(agents, &agent.id, agent.pid, now);
        }
        AgentEvent::ProcessExited { agent } => {
            reduce_process_exited(agents, &agent.id, agent.pid, now);
        }
        AgentEvent::TodosUpdated { agent, todos } => {
            reduce_todos_updated(agents, &agent.id, agent.pid, todos, now);
        }
        AgentEvent::SessionInfo {
            agent,
            session_id,
            branch,
            is_worktree,
            started_at,
        } => {
            reduce_session_info(
                agents,
                overview,
                agent,
                session_id.clone(),
                branch.clone(),
                *is_worktree,
                *started_at,
                now,
            );
        }
    }
    clamp_selection(agents, overview);
}

// ── Activity handlers ───────────────────────────────────────

fn reduce_status_changed(
    agents: &mut [Agent],
    id: &AgentId,
    pid: Option<u32>,
    old: PrimaryStatus,
    new: PrimaryStatus,
    now: DateTime<Local>,
) {
    if let Some(agent) = agent_lookup::find_by_id(agents, id) {
        agent.merge_pid(pid);
        agent.set_status(new);
        agent.touch(now);
        crate::app::push_event(
            agent.recent_events_mut(),
            ActivityEventKind::StatusChanged,
            format!("{} → {}", old.as_str(), new.as_str()),
            now,
        );
    }
}

fn reduce_activity_changed(
    agents: &mut [Agent],
    id: &AgentId,
    pid: Option<u32>,
    activity: Option<nexus_domain::agents::activity::Activity>,
    now: DateTime<Local>,
) {
    if let Some(agent) = agent_lookup::find_by_id(agents, id) {
        agent.merge_pid(pid);
        agent.set_activity(activity);
        agent.touch(now);
    }
}

fn reduce_normalized_state_changed(
    agents: &mut Vec<Agent>,
    id: &AgentId,
    pid: Option<u32>,
    state: NormalizedState,
    now: DateTime<Local>,
) {
    if let Some(agent) = agent_lookup::find_by_id(agents, id) {
        agent.merge_pid(pid);
        agent.set_normalized(state);
        agent.touch(now);
    } else {
        let agent = insert_placeholder_agent(agents, id.clone(), pid, now);
        agent.set_normalized(state);
    }
    // clamp_selection is handled by the top-level reduce()
}

fn reduce_subagent_spawned(
    agents: &mut [Agent],
    id: &AgentId,
    pid: Option<u32>,
    subagent: &Subagent,
    now: DateTime<Local>,
) {
    if let Some(agent) = agent_lookup::find_by_id(agents, id) {
        agent.merge_pid(pid);
        let desc = format!("spawned: {}", subagent.name);
        agent.push_subagent(subagent.clone());
        agent.touch(now);
        crate::app::push_event(
            agent.recent_events_mut(),
            ActivityEventKind::SubagentSpawned,
            desc,
            now,
        );
    }
}

fn reduce_subagent_completed(
    agents: &mut [Agent],
    id: &AgentId,
    pid: Option<u32>,
    subagent_name: &str,
    now: DateTime<Local>,
) {
    if let Some(agent) = agent_lookup::find_by_id(agents, id) {
        agent.merge_pid(pid);
        let tool = id.tool();
        let matching_subagent = agent
            .subagents_mut()
            .iter_mut()
            .find(|s| tool.matches_subagent_name(&s.name, s.description.as_deref(), subagent_name));
        if let Some(sub) = matching_subagent {
            sub.status = PrimaryStatus::Done;
            sub.ended_at = Some(now);
        }
        agent.touch(now);
        crate::app::push_event(
            agent.recent_events_mut(),
            ActivityEventKind::SubagentCompleted,
            format!("completed: {}", subagent_name),
            now,
        );
    }
}

fn reduce_subagents_updated(
    agents: &mut [Agent],
    id: &AgentId,
    pid: Option<u32>,
    subagents: &[Subagent],
    now: DateTime<Local>,
) {
    if let Some(agent) = agent_lookup::find_by_id(agents, id) {
        agent.merge_pid(pid);
        agent.set_subagents(subagents.to_vec());
        if agent.subagents().is_empty() {
            agent.clear_subagent_events();
        }
        agent.touch(now);
    }
}

fn reduce_tool_call_started(
    agents: &mut [Agent],
    id: &AgentId,
    pid: Option<u32>,
    tool_name: &str,
    args_summary: Option<&str>,
    now: DateTime<Local>,
) {
    if let Some(agent) = agent_lookup::find_by_id(agents, id) {
        agent.merge_pid(pid);
        agent.set_activity(Some(nexus_domain::agents::activity::Activity::ToolCall));
        agent.touch(now);
        let desc = match args_summary {
            Some(args) => format!("{} ({})", tool_name, args),
            None => tool_name.to_string(),
        };
        crate::app::push_event(
            agent.recent_events_mut(),
            ActivityEventKind::ToolCall,
            desc,
            now,
        );
    }
}

fn reduce_tool_call_completed(
    agents: &mut [Agent],
    id: &AgentId,
    pid: Option<u32>,
    now: DateTime<Local>,
) {
    if let Some(agent) = agent_lookup::find_by_id(agents, id) {
        agent.merge_pid(pid);
        if agent.activity() == Some(nexus_domain::agents::activity::Activity::ToolCall) {
            agent.set_activity(Some(nexus_domain::agents::activity::Activity::Idle));
        }
        agent.touch(now);
    }
}

fn reduce_todos_updated(
    agents: &mut [Agent],
    id: &AgentId,
    pid: Option<u32>,
    todos: &[nexus_domain::agents::activity::TodoItem],
    now: DateTime<Local>,
) {
    if let Some(agent) = agent_lookup::find_by_id(agents, id) {
        agent.merge_pid(pid);
        agent.set_todos(todos.to_vec());
        agent.touch(now);
    }
}

// ── Lifecycle handler ───────────────────────────────────────

fn reduce_process_exited(
    agents: &mut Vec<Agent>,
    id: &AgentId,
    pid: Option<u32>,
    now: DateTime<Local>,
) {
    // Claude / OpenCode process agents: remove entirely
    if (id.tool() == AgentTool::Claude || id.tool() == AgentTool::OpenCode) && id.is_process() {
        let pid = pid.or_else(|| id.pid());
        if let Some(idx) = agents.iter().position(|a| *a.id() == *id) {
            agents.remove(idx);
            return;
        }
        if let Some(pid) = pid {
            let indices: Vec<usize> = agents
                .iter()
                .enumerate()
                .filter(|(_, a)| a.id().tool() == id.tool() && a.pid() == Some(pid))
                .map(|(idx, _)| idx)
                .collect();
            for idx in indices.into_iter().rev() {
                agents.remove(idx);
            }
        }
        return;
    }

    // Codex process agents: remove process entries, mark session entries exited
    if id.tool() == AgentTool::Codex && id.is_process() {
        let pid = pid.or_else(|| id.pid());

        if let Some(pid) = pid {
            let matching_indices: Vec<usize> = agents
                .iter()
                .enumerate()
                .filter(|(_, a)| a.id().tool() == AgentTool::Codex && a.pid() == Some(pid))
                .map(|(idx, _)| idx)
                .collect();

            if !matching_indices.is_empty() {
                for agent_idx in matching_indices.into_iter().rev() {
                    if agents[agent_idx].id().is_process() {
                        agents.remove(agent_idx);
                    } else {
                        mark_agent_exited(&mut agents[agent_idx], Some(pid), now);
                    }
                }
                return;
            }
        }
    }

    // Fallback: find by id, then by pid
    if let Some(agent) = agent_lookup::find_by_id(agents, id) {
        mark_agent_exited(agent, pid, now);
    } else if let Some(pid) = pid {
        if id.tool() == AgentTool::Codex {
            let matching_indices: Vec<usize> = agents
                .iter()
                .enumerate()
                .filter(|(_, a)| a.id().tool() == id.tool() && a.pid() == Some(pid))
                .map(|(idx, _)| idx)
                .collect();

            if !matching_indices.is_empty() {
                for agent_idx in matching_indices {
                    mark_agent_exited(&mut agents[agent_idx], Some(pid), now);
                }
                return;
            }
        }

        if let Some(agent) = agent_lookup::find_by_pid(agents, id.tool(), pid) {
            mark_agent_exited(agent, Some(pid), now);
        }
    }
}

// ── Discovery handlers ──────────────────────────────────────

fn reduce_discovered(
    agents: &mut Vec<Agent>,
    overview: &mut OverviewState,
    id: &AgentId,
    pid: Option<u32>,
    cwd: &Path,
    started_at: Option<DateTime<Local>>,
    now: DateTime<Local>,
) {
    let pid = pid.or_else(|| id.pid());
    let preserve_codex_session_status = id.tool() == AgentTool::Codex && id.is_process();

    // Exact id match
    if let Some(agent_idx) = agents.iter().position(|a| *a.id() == *id) {
        let absorbed_pid = {
            let agent = &mut agents[agent_idx];
            agent.set_cwd(cwd.to_path_buf());
            agent.merge_pid(pid);
            if !(preserve_codex_session_status && agent.id().is_session()) {
                agent.set_status(PrimaryStatus::Running);
            }
            if let Some(real_start) = started_at {
                agent.clamp_started_at(real_start);
            }
            agent.touch(now);
            agent.pid()
        };
        if id.tool() == AgentTool::Codex && id.is_session() {
            agent_lookup::remove_redundant_codex_process_agents(agents, absorbed_pid);
        }
        return;
    }

    // Codex: try matching by pid/cwd
    let session_id = id.session_id().map(str::to_string);
    if id.tool() == AgentTool::Codex {
        if let Some(pid_val) = pid {
            let matching_agent_indices =
                agent_lookup::session_indices_by_pid(agents, id.tool(), pid_val, cwd);
            if matching_agent_indices.len() > 1 {
                for agent_idx in matching_agent_indices {
                    let agent = &mut agents[agent_idx];
                    agent.set_cwd(cwd.to_path_buf());
                    agent.merge_pid(Some(pid_val));
                    if !preserve_codex_session_status {
                        agent.set_status(PrimaryStatus::Running);
                    }
                    if let Some(real_start) = started_at {
                        agent.clamp_started_at(real_start);
                    }
                    agent.touch(now);
                }
                return;
            }
            if let Some(agent_idx) = matching_agent_indices.into_iter().next() {
                let agent = &mut agents[agent_idx];
                agent.set_cwd(cwd.to_path_buf());
                agent.merge_pid(Some(pid_val));
                if !preserve_codex_session_status {
                    agent.set_status(PrimaryStatus::Running);
                }
                if let Some(real_start) = started_at {
                    agent.clamp_started_at(real_start);
                }
                agent.touch(now);
                return;
            }

            if id.is_process() {
                let exited_agent_indices =
                    agent_lookup::exited_session_indices_by_pid(agents, id.tool(), pid_val, cwd);
                if !exited_agent_indices.is_empty() {
                    for agent_idx in exited_agent_indices {
                        let agent = &mut agents[agent_idx];
                        agent.set_cwd(cwd.to_path_buf());
                        agent.merge_pid(Some(pid_val));
                        agent.set_status(PrimaryStatus::Running);
                        if let Some(real_start) = started_at {
                            agent.clamp_started_at(real_start);
                        }
                        agent.touch(now);
                    }
                    return;
                }
            }
        }
    }

    // No match: create new agent
    let normalized = if id.tool() == AgentTool::Codex {
        NormalizedState::new(NormalizedStatus::Unknown, None)
    } else {
        NormalizedState::new(NormalizedStatus::Working, None)
    };
    agents.push(Agent {
        ident: AgentIdent {
            id: id.clone(),
            pid,
            session_id,
            alias: None,
            cwd: Some(cwd.to_path_buf()),
            branch: None,
            is_worktree: false,
            started_at: started_at.unwrap_or(now),
        },
        lifecycle: AgentLifecycle {
            status: PrimaryStatus::Running,
            normalized,
            activity: None,
            last_activity: now,
        },
        detail: AgentDetail {
            subagents: Vec::new(),
            recent_events: VecDeque::new(),
            todos: Vec::new(),
        },
    });
    if overview.selected_index.is_none() {
        overview.selected_index = Some(0);
    }
}

#[allow(clippy::too_many_arguments)]
fn reduce_session_info(
    agents: &mut Vec<Agent>,
    overview: &mut OverviewState,
    agent: &AgentRef,
    session_id: Option<String>,
    branch: Option<String>,
    is_worktree: bool,
    started_at: Option<DateTime<Local>>,
    now: DateTime<Local>,
) {
    let pid = agent.pid.or_else(|| agent.id.pid());

    if agent.id.is_session() {
        if let Some(a) = agent_lookup::find_by_id(agents, &agent.id) {
            apply_session_info(
                a,
                pid,
                session_id,
                branch,
                is_worktree,
                started_at,
                now,
            );
            let absorbed_pid = a.pid();
            agent_lookup::remove_redundant_codex_process_agents(agents, absorbed_pid);
            return;
        }

        if let Some(pid_val) = pid {
            if let Some(a) = agent_lookup::find_process_agent(agents, agent.id.tool(), pid_val) {
                a.set_id(agent.id.clone());
                apply_session_info(
                    a,
                    Some(pid_val),
                    session_id,
                    branch,
                    is_worktree,
                    started_at,
                    now,
                );
                agent_lookup::remove_redundant_codex_process_agents(agents, Some(pid_val));
                return;
            }
        }

        let a = insert_placeholder_agent(agents, agent.id.clone(), pid, now);
        apply_session_info(a, pid, session_id, branch, is_worktree, started_at, now);
        agent_lookup::remove_redundant_codex_process_agents(agents, pid);
        // Selection set by insert_placeholder_agent if needed; top-level clamp handles the rest
        if overview.selected_index.is_none() {
            overview.selected_index = Some(0);
        }
        return;
    }

    // Non-session id
    if let Some(a) = agent_lookup::find_by_id(agents, &agent.id) {
        apply_session_info(a, pid, session_id, branch, is_worktree, started_at, now);
    } else {
        let a = insert_placeholder_agent(agents, agent.id.clone(), pid, now);
        apply_session_info(a, pid, session_id, branch, is_worktree, started_at, now);
    }
}

// ── Store helpers ───────────────────────────────────────────

fn insert_placeholder_agent(
    agents: &mut Vec<Agent>,
    id: AgentId,
    pid: Option<u32>,
    now: DateTime<Local>,
) -> &mut Agent {
    if let Some(agent_idx) = agents.iter().position(|agent| *agent.id() == id) {
        return &mut agents[agent_idx];
    }

    let session_id = id.session_id().map(str::to_string);
    let normalized = if id.tool() == AgentTool::Codex {
        NormalizedState::new(NormalizedStatus::Unknown, None)
    } else {
        NormalizedState::new(NormalizedStatus::Working, None)
    };
    let idx = agents.len();
    agents.push(Agent {
        ident: AgentIdent {
            id,
            pid,
            session_id,
            alias: None,
            cwd: None,
            branch: None,
            is_worktree: false,
            started_at: now,
        },
        lifecycle: AgentLifecycle {
            status: PrimaryStatus::Running,
            normalized,
            activity: None,
            last_activity: now,
        },
        detail: AgentDetail {
            subagents: Vec::new(),
            recent_events: VecDeque::new(),
            todos: Vec::new(),
        },
    });
    &mut agents[idx]
}

fn mark_agent_exited(agent: &mut Agent, pid: Option<u32>, now: DateTime<Local>) {
    agent.merge_pid(pid);
    agent.set_status(PrimaryStatus::Exited);
    agent.set_activity(None);
    agent.touch(now);
    crate::app::push_event(
        agent.recent_events_mut(),
        ActivityEventKind::StatusChanged,
        "Process exited".to_string(),
        now,
    );
}

fn apply_session_info(
    agent: &mut Agent,
    pid: Option<u32>,
    session_id: Option<String>,
    branch: Option<String>,
    is_worktree: bool,
    started_at: Option<DateTime<Local>>,
    now: DateTime<Local>,
) {
    agent.merge_pid(pid);
    agent.set_session_id(session_id);
    agent.set_branch(branch);
    agent.set_is_worktree(is_worktree);
    if let Some(real_start) = started_at {
        agent.clamp_started_at(real_start);
    }
    agent.touch(now);
}

fn clamp_selection(agents: &[Agent], overview: &mut OverviewState) {
    let count = crate::app::projection::visible_count(agents, overview);
    overview.clamp_selection(count);
}

#[cfg(test)]
#[path = "agents_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "proptest_agents.rs"]
mod proptest_tests;
