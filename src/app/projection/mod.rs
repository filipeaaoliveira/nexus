use std::borrow::Cow;

use crate::app::state::OverviewState;
use crate::domain::agents::activity::Activity;
use crate::domain::agents::model::{Agent, SortMode};
use crate::domain::agents::status::NormalizedStatus;
#[cfg(test)]
use crate::domain::agents::status::PrimaryStatus;

/// A projected view of an `Agent` for UI rendering.
///
/// Pre-computes display values so render functions don't recompute them.
#[derive(Debug, Clone)]
pub struct AgentView<'a> {
    pub agent: &'a Agent,
    pub display_name: Cow<'a, str>,
    pub status_label: Cow<'a, str>,
}

impl<'a> AgentView<'a> {
    pub fn from_agent(agent: &'a Agent) -> Self {
        let display_name = Cow::Borrowed(agent.display_name());
        let status_label = Cow::Owned(format_status_label(agent));
        Self {
            agent,
            display_name,
            status_label,
        }
    }
}

fn format_status_label(agent: &Agent) -> String {
    match &agent.normalized.reason {
        Some(reason) => format!("{} · {}", agent.normalized.status.as_str(), reason.as_str()),
        None if agent.normalized.status == NormalizedStatus::Working
            && agent.activity == Some(Activity::ToolCall) =>
        {
            format!("{} · tool call", agent.normalized.status.as_str())
        }
        None if agent.normalized.status == NormalizedStatus::Working
            && agent.activity == Some(Activity::Compacting) =>
        {
            format!("{} · compacting", agent.normalized.status.as_str())
        }
        None => agent.normalized.status.as_str().to_string(),
    }
}

pub fn project_visible_agents<'a>(
    agents: &'a [Agent],
    overview: &OverviewState,
) -> Vec<AgentView<'a>> {
    visible_agents(agents, overview)
        .into_iter()
        .map(AgentView::from_agent)
        .collect()
}

pub fn visible_agent_indices(agents: &[Agent], overview: &OverviewState) -> Vec<usize> {
    let mut indices: Vec<usize> = agents
        .iter()
        .enumerate()
        .filter(|(_, agent)| {
            !(agent.id.tool() == crate::domain::agents::ids::AgentTool::Codex
                && agent.id.is_session()
                && agent.pid.is_none())
        })
        .map(|(idx, _)| idx)
        .collect();

    if let Some(filter) = overview.filter.as_deref() {
        let filter = filter.to_lowercase();
        indices.retain(|&idx| agent_matches_filter(&agents[idx], &filter));
    }

    match overview.sort_mode {
        SortMode::Urgency => {
            indices.sort_by_key(|&idx| {
                let agent = &agents[idx];
                (std::cmp::Reverse(agent.normalized.status), agent.status)
            });
        }
        SortMode::Name => {
            indices.sort_by(|&left, &right| {
                let a = &agents[left];
                let b = &agents[right];
                a.display_name()
                    .cmp(b.display_name())
                    .then_with(|| a.pid.cmp(&b.pid))
                    .then_with(|| a.id.session_id().cmp(&b.id.session_id()))
            });
        }
        SortMode::Elapsed => {
            indices.sort_by(|&left, &right| agents[left].started_at.cmp(&agents[right].started_at));
        }
    }

    indices
}

pub fn visible_agents<'a>(agents: &'a [Agent], overview: &OverviewState) -> Vec<&'a Agent> {
    visible_agent_indices(agents, overview)
        .into_iter()
        .map(|idx| &agents[idx])
        .collect()
}

pub fn selected_agent_id(
    agents: &[Agent],
    overview: &OverviewState,
) -> Option<crate::domain::agents::ids::AgentId> {
    let visible_idx = overview.selected_index()?;
    let real_idx = *visible_agent_indices(agents, overview).get(visible_idx)?;
    Some(agents[real_idx].id.clone())
}

#[cfg(test)]
pub fn count_by_status(agents: &[Agent], overview: &OverviewState, status: PrimaryStatus) -> usize {
    visible_agents(agents, overview)
        .into_iter()
        .filter(|agent| agent.status == status)
        .count()
}

fn agent_matches_filter(agent: &Agent, filter: &str) -> bool {
    agent.display_name().to_lowercase().contains(filter)
        || agent.status.as_str().to_lowercase().contains(filter)
        || agent
            .normalized
            .status
            .as_str()
            .to_lowercase()
            .contains(filter)
        || agent
            .normalized
            .reason
            .as_ref()
            .map(|reason| reason.as_str().to_lowercase().contains(filter))
            .unwrap_or(false)
        || agent
            .cwd
            .as_deref()
            .map(|cwd| cwd.to_string_lossy().to_lowercase().contains(filter))
            .unwrap_or(false)
        || agent
            .branch
            .as_deref()
            .map(|branch| branch.to_lowercase().contains(filter))
            .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::AppState;
    use crate::domain::agents::activity::Activity;
    use crate::domain::agents::ids::{AgentId, AgentTool};
    use crate::domain::agents::status::{NormalizedReason, NormalizedState, NormalizedStatus};
    use crate::event::AgentEvent;
    use std::path::PathBuf;

    #[test]
    fn project_visible_agents_produces_views_without_mutating_store() {
        let mut state = AppState::new();
        state.handle_event(AgentEvent::Discovered {
            id: AgentId::new(AgentTool::Claude, 10),
            pid: Some(10),
            cwd: PathBuf::from("/tmp/claude"),
            started_at: None,
        });
        state.handle_event(AgentEvent::Discovered {
            id: AgentId::new(AgentTool::OpenCode, 11),
            pid: Some(11),
            cwd: PathBuf::from("/tmp/opencode"),
            started_at: None,
        });

        let agents_before = state.agents().len();
        let views = project_visible_agents(state.agents(), state.root().overview());
        let agents_after = state.agents().len();

        assert_eq!(agents_before, agents_after);
        assert_eq!(views.len(), 2);
        assert_eq!(
            views[0].display_name.as_ref(),
            views[0].agent.display_name()
        );
    }

    #[test]
    fn agent_view_status_label_includes_reason_when_present() {
        let mut state = AppState::new();
        state.handle_event(AgentEvent::Discovered {
            id: AgentId::new(AgentTool::Claude, 10),
            pid: Some(10),
            cwd: PathBuf::from("/tmp/claude"),
            started_at: None,
        });
        state.handle_event(AgentEvent::NormalizedStateChanged {
            id: AgentId::new(AgentTool::Claude, 10),
            pid: Some(10),
            state: NormalizedState::new(
                NormalizedStatus::NeedsInput,
                Some(NormalizedReason::Review),
            ),
        });

        let views = project_visible_agents(state.agents(), state.root().overview());
        assert_eq!(views[0].status_label.as_ref(), "needs input · review");
    }

    #[test]
    fn agent_view_status_label_shows_tool_call_hint() {
        let mut state = AppState::new();
        state.handle_event(AgentEvent::Discovered {
            id: AgentId::new(AgentTool::Claude, 10),
            pid: Some(10),
            cwd: PathBuf::from("/tmp/claude"),
            started_at: None,
        });
        state.handle_event(AgentEvent::NormalizedStateChanged {
            id: AgentId::new(AgentTool::Claude, 10),
            pid: Some(10),
            state: NormalizedState::new(NormalizedStatus::Working, None),
        });
        state.handle_event(AgentEvent::ActivityChanged {
            id: AgentId::new(AgentTool::Claude, 10),
            pid: Some(10),
            activity: Some(Activity::ToolCall),
        });

        let views = project_visible_agents(state.agents(), state.root().overview());
        assert_eq!(views[0].status_label.as_ref(), "working · tool call");
    }

    #[test]
    fn overview_projection_sorts_by_urgency_without_mutating_agent_store() {
        let mut state = AppState::new();
        state.handle_event(AgentEvent::Discovered {
            id: AgentId::new(AgentTool::Claude, 10),
            pid: Some(10),
            cwd: PathBuf::from("/tmp/claude"),
            started_at: None,
        });
        state.handle_event(AgentEvent::Discovered {
            id: AgentId::new(AgentTool::OpenCode, 11),
            pid: Some(11),
            cwd: PathBuf::from("/tmp/opencode"),
            started_at: None,
        });
        state.handle_event(AgentEvent::NormalizedStateChanged {
            id: AgentId::new(AgentTool::OpenCode, 11),
            pid: Some(11),
            state: NormalizedState::new(
                NormalizedStatus::NeedsInput,
                Some(NormalizedReason::Review),
            ),
        });

        let agents_before: Vec<_> = state.agents().iter().map(|a| a.id.clone()).collect();
        let views = project_visible_agents(state.agents(), state.root().overview());
        let agents_after: Vec<_> = state.agents().iter().map(|a| a.id.clone()).collect();

        // Store order unchanged
        assert_eq!(agents_before, agents_after);
        // Both agents visible, views are sorted projections
        assert_eq!(views.len(), 2);
        // NeedsInput (4) has highest urgency value, Reverse sort puts it first
        let tools: Vec<_> = views.iter().map(|v| v.agent.id.tool()).collect();
        assert!(
            tools.contains(&AgentTool::Claude) && tools.contains(&AgentTool::OpenCode),
            "both agents visible in projection"
        );
    }
}
