use std::path::PathBuf;

use crate::app::action::{DetectorAction, OverviewAction};
use crate::app::effect::Effect;
use crate::app::state::{OverviewAgent, OverviewMode, OverviewState};
use crate::types::{AgentId, AgentTool, NormalizedState, NormalizedStatus, PrimaryStatus};

pub fn reduce(state: &mut OverviewState, action: &OverviewAction) -> Vec<Effect> {
    match action {
        OverviewAction::SelectNext => select_next(state),
        OverviewAction::SelectPrevious => select_previous(state),
        OverviewAction::CycleSortMode => {
            state.sort_mode = state.sort_mode.next();
            state.clamp_selection();
        }
        OverviewAction::StartFiltering => {
            state.mode = OverviewMode::Filtering {
                input: String::new(),
            };
            state.filter = None;
            state.clamp_selection();
        }
        OverviewAction::UpdateFilter(input) => {
            state.mode = OverviewMode::Filtering {
                input: input.clone(),
            };
            state.filter = if input.is_empty() {
                None
            } else {
                Some(input.clone())
            };
            state.clamp_selection();
        }
        OverviewAction::ConfirmFilter => state.mode = OverviewMode::Normal,
        OverviewAction::ClearFilter => {
            state.mode = OverviewMode::Normal;
            state.filter = None;
            state.clamp_selection();
        }
        OverviewAction::StartRenaming { agent_id } => {
            if state
                .visible_agents()
                .iter()
                .any(|agent| agent.id == *agent_id)
            {
                state.mode = OverviewMode::Renaming {
                    agent_id: agent_id.clone(),
                    input: String::new(),
                };
            }
        }
        OverviewAction::UpdateRenameInput(input) => {
            if let OverviewMode::Renaming { agent_id, .. } = &state.mode {
                state.mode = OverviewMode::Renaming {
                    agent_id: agent_id.clone(),
                    input: input.clone(),
                };
            }
        }
        OverviewAction::DismissExited => {
            state
                .agents
                .retain(|agent| agent.status != PrimaryStatus::Exited);
            state.clamp_selection();
        }
        OverviewAction::DismissSelected => {
            if let Some(selected_id) = state.selected_agent_id() {
                if let Some(agent_idx) = state
                    .agents
                    .iter()
                    .position(|agent| agent.id == selected_id)
                {
                    state.agents.remove(agent_idx);
                    state.clamp_selection();
                }
            }
        }
        OverviewAction::SetMode(mode) => {
            state.mode = mode.clone();
            if let OverviewMode::Filtering { input } = &state.mode {
                state.filter = if input.is_empty() {
                    None
                } else {
                    Some(input.clone())
                };
            }
            state.clamp_selection();
        }
        OverviewAction::ConfirmRename => confirm_rename(state),
        OverviewAction::CancelMode => cancel_mode(state),
    }

    Vec::new()
}

pub fn reduce_detector(state: &mut OverviewState, action: &DetectorAction) -> Vec<Effect> {
    match action {
        DetectorAction::Discovered {
            id,
            pid,
            cwd,
            started_at,
        } => handle_discovered(state, id.clone(), *pid, cwd.clone(), *started_at),
        DetectorAction::SessionInfo {
            id,
            pid,
            session_id,
            branch,
            is_worktree,
            started_at,
        } => handle_session_info(
            state,
            id.clone(),
            *pid,
            session_id.clone(),
            branch.clone(),
            *is_worktree,
            *started_at,
        ),
        DetectorAction::StatusChanged { id, pid, new, .. } => {
            handle_status_changed(state, id, *pid, *new)
        }
        DetectorAction::NormalizedStateChanged {
            id,
            pid,
            state: normalized,
        } => handle_normalized_state_changed(state, id.clone(), *pid, normalized.clone()),
        DetectorAction::ProcessExited { id, pid } => handle_process_exited(state, id, *pid),
        DetectorAction::HealthChanged { .. } => {}
    }

    Vec::new()
}

fn select_next(state: &mut OverviewState) {
    let len = state.visible_agent_count();
    if len == 0 {
        return;
    }

    state.selected_index = Some(match state.selected_index {
        Some(idx) => (idx + 1) % len,
        None => 0,
    });
}

fn select_previous(state: &mut OverviewState) {
    let len = state.visible_agent_count();
    if len == 0 {
        return;
    }

    state.selected_index = Some(match state.selected_index {
        Some(idx) => (idx + len - 1) % len,
        None => len - 1,
    });
}

fn confirm_rename(state: &mut OverviewState) {
    let OverviewMode::Renaming { agent_id, input } = &state.mode else {
        return;
    };

    if !input.is_empty() {
        if let Some(agent) = state.agents.iter_mut().find(|agent| agent.id == *agent_id) {
            agent.alias = Some(input.clone());
        }
    }

    state.mode = OverviewMode::Normal;
}

fn cancel_mode(state: &mut OverviewState) {
    if matches!(state.mode, OverviewMode::Filtering { .. }) {
        state.filter = None;
    }
    state.mode = OverviewMode::Normal;
    state.clamp_selection();
}

fn handle_discovered(
    state: &mut OverviewState,
    id: AgentId,
    pid: Option<u32>,
    cwd: PathBuf,
    started_at: Option<chrono::DateTime<chrono::Local>>,
) {
    let pid = pid.or_else(|| id.pid());
    let preserve_codex_session_status = id.tool() == AgentTool::Codex && id.is_process();

    if let Some(agent) = state.agents.iter_mut().find(|agent| agent.id == id) {
        agent.cwd = cwd;
        agent.pid = pid.or(agent.pid);
        if !(preserve_codex_session_status && agent.id.is_session()) {
            agent.status = PrimaryStatus::Running;
        }
        if started_at.is_some() && (agent.started_at.is_none() || started_at < agent.started_at) {
            agent.started_at = started_at;
        }
        state.clamp_selection();
        return;
    }

    if id.tool() == AgentTool::Codex {
        if let Some(pid) = pid {
            let matching_agent_indices: Vec<_> = state
                .agents
                .iter()
                .enumerate()
                .filter(|(_, agent)| {
                    agent.id.tool() == id.tool()
                        && agent.id.is_session()
                        && agent.pid == Some(pid)
                        && agent.status != PrimaryStatus::Exited
                        && (agent.cwd.as_os_str().is_empty() || agent.cwd == cwd)
                })
                .map(|(idx, _)| idx)
                .collect();

            if !matching_agent_indices.is_empty() {
                for agent_idx in matching_agent_indices {
                    let agent = &mut state.agents[agent_idx];
                    agent.cwd = cwd.clone();
                    agent.pid = Some(pid);
                    if !preserve_codex_session_status {
                        agent.status = PrimaryStatus::Running;
                    }
                    if started_at.is_some()
                        && (agent.started_at.is_none() || started_at < agent.started_at)
                    {
                        agent.started_at = started_at;
                    }
                }
                state.clamp_selection();
                return;
            }

            if id.is_process() {
                if let Some(agent) = state.agents.iter_mut().find(|agent| {
                    agent.id.tool() == id.tool()
                        && agent.id.is_session()
                        && agent.pid == Some(pid)
                        && agent.status == PrimaryStatus::Exited
                        && agent.cwd == cwd
                }) {
                    agent.cwd = cwd;
                    agent.pid = Some(pid);
                    agent.status = PrimaryStatus::Running;
                    if started_at.is_some()
                        && (agent.started_at.is_none() || started_at < agent.started_at)
                    {
                        agent.started_at = started_at;
                    }
                    state.clamp_selection();
                    return;
                }
            }
        }
    }

    state.agents.push(OverviewAgent {
        session_id: id.session_id().map(str::to_string),
        id: id.clone(),
        pid,
        status: PrimaryStatus::Running,
        normalized: initial_normalized_state(&id),
        cwd,
        branch: None,
        is_worktree: false,
        started_at,
        alias: None,
    });
    if state.selected_index.is_none() {
        state.selected_index = Some(0);
    }
    state.clamp_selection();
}

fn handle_session_info(
    state: &mut OverviewState,
    id: AgentId,
    pid: Option<u32>,
    session_id: String,
    branch: Option<String>,
    is_worktree: bool,
    started_at: Option<chrono::DateTime<chrono::Local>>,
) {
    let pid = pid.or_else(|| id.pid());

    if id.is_session() {
        if let Some(agent) = state.agents.iter_mut().find(|agent| agent.id == id) {
            apply_session_info(
                agent,
                pid,
                &session_id,
                branch.clone(),
                is_worktree,
                started_at,
            );
            remove_redundant_codex_process_agents(state, pid);
            state.clamp_selection();
            return;
        }

        if let Some(pid) = pid {
            if let Some(agent) = state.agents.iter_mut().find(|agent| {
                agent.id.tool() == id.tool() && agent.pid == Some(pid) && agent.id.is_process()
            }) {
                agent.id = id;
                apply_session_info(
                    agent,
                    Some(pid),
                    &session_id,
                    branch,
                    is_worktree,
                    started_at,
                );
                remove_redundant_codex_process_agents(state, Some(pid));
                state.clamp_selection();
                return;
            }
        }
    } else if let Some(agent) = state.agents.iter_mut().find(|agent| agent.id == id) {
        apply_session_info(agent, pid, &session_id, branch, is_worktree, started_at);
        state.clamp_selection();
        return;
    }

    let mut agent = placeholder_agent(id, pid);
    apply_session_info(
        &mut agent,
        pid,
        &session_id,
        branch,
        is_worktree,
        started_at,
    );
    let remove_codex_process = agent.id.is_session();
    state.agents.push(agent);
    if remove_codex_process {
        remove_redundant_codex_process_agents(state, pid);
    }
    state.clamp_selection();
}

fn handle_status_changed(
    state: &mut OverviewState,
    id: &AgentId,
    pid: Option<u32>,
    new: PrimaryStatus,
) {
    if let Some(agent) = state.agents.iter_mut().find(|agent| agent.id == *id) {
        agent.pid = pid.or(agent.pid);
        agent.status = new;
        state.clamp_selection();
    }
}

fn handle_normalized_state_changed(
    state: &mut OverviewState,
    id: AgentId,
    pid: Option<u32>,
    normalized: NormalizedState,
) {
    if let Some(agent) = state.agents.iter_mut().find(|agent| agent.id == id) {
        agent.pid = pid.or(agent.pid);
        agent.normalized = normalized;
    } else {
        let mut agent = placeholder_agent(id, pid);
        agent.normalized = normalized;
        state.agents.push(agent);
    }
    state.clamp_selection();
}

fn handle_process_exited(state: &mut OverviewState, id: &AgentId, pid: Option<u32>) {
    if (id.tool() == AgentTool::Claude || id.tool() == AgentTool::OpenCode) && id.is_process() {
        let pid = pid.or_else(|| id.pid());
        if let Some(agent_idx) = state.agents.iter().position(|agent| agent.id == *id) {
            state.agents.remove(agent_idx);
        } else if let Some(pid) = pid {
            state
                .agents
                .retain(|agent| !(agent.id.tool() == id.tool() && agent.pid == Some(pid)));
        }
        state.clamp_selection();
        return;
    }

    if id.tool() == AgentTool::Codex && id.is_process() {
        let pid = pid.or_else(|| id.pid());
        if let Some(pid) = pid {
            let mut removed_any = false;
            for index in (0..state.agents.len()).rev() {
                if state.agents[index].id.tool() == AgentTool::Codex
                    && state.agents[index].pid == Some(pid)
                {
                    if state.agents[index].id.is_process() {
                        state.agents.remove(index);
                    } else {
                        state.agents[index].status = PrimaryStatus::Exited;
                    }
                    removed_any = true;
                }
            }
            if removed_any {
                state.clamp_selection();
                return;
            }
        }
    }

    if let Some(agent) = state.agents.iter_mut().find(|agent| agent.id == *id) {
        agent.pid = pid.or(agent.pid);
        agent.status = PrimaryStatus::Exited;
        state.clamp_selection();
    }
}

fn apply_session_info(
    agent: &mut OverviewAgent,
    pid: Option<u32>,
    session_id: &str,
    branch: Option<String>,
    is_worktree: bool,
    started_at: Option<chrono::DateTime<chrono::Local>>,
) {
    agent.pid = pid.or(agent.pid);
    if !session_id.is_empty() {
        agent.session_id = Some(session_id.to_string());
    }
    if branch.is_some() {
        agent.branch = branch;
    }
    agent.is_worktree = is_worktree;
    if started_at.is_some() && (agent.started_at.is_none() || started_at < agent.started_at) {
        agent.started_at = started_at;
    }
}

fn remove_redundant_codex_process_agents(state: &mut OverviewState, pid: Option<u32>) {
    let Some(pid) = pid else {
        return;
    };

    state.agents.retain(|agent| {
        !(agent.id.tool() == AgentTool::Codex && agent.id.is_process() && agent.pid == Some(pid))
    });
}

fn placeholder_agent(id: AgentId, pid: Option<u32>) -> OverviewAgent {
    OverviewAgent {
        session_id: id.session_id().map(str::to_string),
        id: id.clone(),
        pid: pid.or_else(|| id.pid()),
        status: PrimaryStatus::Running,
        normalized: initial_normalized_state(&id),
        cwd: PathBuf::new(),
        branch: None,
        is_worktree: false,
        started_at: None,
        alias: None,
    }
}

fn initial_normalized_state(id: &AgentId) -> NormalizedState {
    if id.tool() == AgentTool::Codex {
        NormalizedState::new(NormalizedStatus::Unknown, None)
    } else {
        NormalizedState::new(NormalizedStatus::Working, None)
    }
}
