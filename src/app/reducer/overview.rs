use crate::app::action::OverviewAction;
use crate::app::agent::Agent;
use crate::app::effect::Effect;
use crate::app::state::{OverviewMode, OverviewState};
use nexus_domain::agents::status::PrimaryStatus;

pub fn reduce(
    agents: &mut Vec<Agent>,
    state: &mut OverviewState,
    action: &OverviewAction,
) -> Vec<Effect> {
    match action {
        OverviewAction::SelectNext => {
            select_next(agents, state);
        }
        OverviewAction::SelectPrevious => {
            select_previous(agents, state);
        }
        OverviewAction::CycleSortMode => {
            state.sort_mode = state.sort_mode.next();
            clamp_selection(agents, state);
        }
        OverviewAction::StartFiltering => {
            state.mode = OverviewMode::Filtering {
                input: String::new(),
            };
            state.filter = None;
            clamp_selection(agents, state);
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
            clamp_selection(agents, state);
        }
        OverviewAction::ConfirmFilter => {
            state.mode = OverviewMode::Normal;
            clamp_selection(agents, state);
        }
        #[cfg(test)]
        OverviewAction::ClearFilter => {
            state.mode = OverviewMode::Normal;
            state.filter = None;
            clamp_selection(agents, state);
        }
        OverviewAction::StartRenaming { agent_id } => {
            // Validate agent exists in visible list
            let visible = crate::app::projection::visible_agents(agents, state);
            if visible.iter().any(|agent| *agent.id() == *agent_id) {
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
            agents.retain(|agent| agent.status() != PrimaryStatus::Exited);
            clamp_selection(agents, state);
        }
        OverviewAction::DismissSelected => {
            if let Some(selected_id) = crate::app::projection::selected_agent_id(agents, state) {
                agents.retain(|agent| *agent.id() != selected_id);
                clamp_selection(agents, state);
            }
        }
        OverviewAction::ConfirmRename => {
            if let OverviewMode::Renaming { agent_id, input } = &state.mode {
                if !input.is_empty() {
                    if let Some(agent) = agents.iter_mut().find(|agent| *agent.id() == *agent_id) {
                        agent.set_alias(Some(input.clone()));
                    }
                }
            }
            state.mode = OverviewMode::Normal;
            clamp_selection(agents, state);
        }
        OverviewAction::CancelMode => {
            let was_filtering = matches!(state.mode, OverviewMode::Filtering { .. });
            state.mode = OverviewMode::Normal;
            if was_filtering {
                state.filter = None;
            }
            clamp_selection(agents, state);
        }
    }

    Vec::new()
}

fn visible_count(agents: &[Agent], state: &OverviewState) -> usize {
    crate::app::projection::visible_agents(agents, state).len()
}

fn clamp_selection(agents: &[Agent], state: &mut OverviewState) {
    state.clamp_selection(visible_count(agents, state));
}

fn select_next(agents: &[Agent], state: &mut OverviewState) {
    let len = visible_count(agents, state);
    if len == 0 {
        return;
    }
    state.selected_index = Some(match state.selected_index {
        Some(idx) => (idx + 1) % len,
        None => 0,
    });
}

fn select_previous(agents: &[Agent], state: &mut OverviewState) {
    let len = visible_count(agents, state);
    if len == 0 {
        return;
    }
    state.selected_index = Some(match state.selected_index {
        Some(idx) => (idx + len - 1) % len,
        None => len - 1,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::agent::{Agent, AgentDetail, AgentIdent, AgentLifecycle};
    use chrono::Local;
    use nexus_domain::agents::ids::{AgentId, AgentTool};
    use nexus_domain::agents::status::{NormalizedState, NormalizedStatus, PrimaryStatus};
    use std::collections::VecDeque;
    use std::path::PathBuf;

    fn make_agent(tool: AgentTool, pid: u32, status: PrimaryStatus) -> Agent {
        let now = Local::now();
        Agent {
            ident: AgentIdent {
                id: AgentId::new(tool, pid),
                pid: Some(pid),
                session_id: None,
                alias: None,
                cwd: Some(PathBuf::from(format!("/tmp/{pid}"))),
                branch: None,
                is_worktree: false,
                started_at: now,
            },
            lifecycle: AgentLifecycle {
                status,
                normalized: NormalizedState::new(NormalizedStatus::Working, None),
                activity: None,
                last_activity: now,
            },
            detail: AgentDetail {
                subagents: Vec::new(),
                recent_events: VecDeque::new(),
                todos: Vec::new(),
            },
        }
    }

    #[test]
    fn dismiss_exited_removes_exited_agents_and_clamps_selection() {
        let mut agents = vec![
            make_agent(AgentTool::Claude, 1, PrimaryStatus::Running),
            make_agent(AgentTool::Claude, 2, PrimaryStatus::Exited),
        ];
        let mut state = OverviewState { selected_index: Some(1), ..Default::default() };
        reduce(&mut agents, &mut state, &OverviewAction::DismissExited);
        assert_eq!(agents.len(), 1);
        assert_eq!(agents[0].pid(), Some(1));
        assert_eq!(state.selected_index, Some(0));
    }

    #[test]
    fn dismiss_selected_removes_agent_at_selection() {
        let mut agents = vec![
            make_agent(AgentTool::Claude, 1, PrimaryStatus::Running),
            make_agent(AgentTool::Claude, 2, PrimaryStatus::Running),
        ];
        let mut state = OverviewState { selected_index: Some(0), ..Default::default() };
        reduce(&mut agents, &mut state, &OverviewAction::DismissSelected);
        assert_eq!(agents.len(), 1);
        assert_eq!(agents[0].pid(), Some(2));
    }

    #[test]
    fn confirm_rename_applies_alias_to_agent() {
        let mut agents = vec![make_agent(AgentTool::Claude, 1, PrimaryStatus::Running)];
        let agent_id = agents[0].id().clone();
        let mut state = OverviewState {
            selected_index: Some(0),
            mode: OverviewMode::Renaming {
                agent_id,
                input: "my-agent".to_string(),
            },
            ..Default::default()
        };
        reduce(&mut agents, &mut state, &OverviewAction::ConfirmRename);
        assert_eq!(agents[0].alias(), Some("my-agent"));
        assert_eq!(state.mode, OverviewMode::Normal);
    }

    #[test]
    fn select_next_wraps_around() {
        let mut agents = vec![
            make_agent(AgentTool::Claude, 1, PrimaryStatus::Running),
            make_agent(AgentTool::Claude, 2, PrimaryStatus::Running),
        ];
        let mut state = OverviewState { selected_index: Some(1), ..Default::default() };
        reduce(&mut agents, &mut state, &OverviewAction::SelectNext);
        assert_eq!(state.selected_index, Some(0));
    }
}
