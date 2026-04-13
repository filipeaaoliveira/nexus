use crate::app::state::OverviewState;
use crate::domain::agents::model::{Agent, SortMode};
#[cfg(test)]
use crate::domain::agents::status::PrimaryStatus;

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

pub fn selected_agent_id(agents: &[Agent], overview: &OverviewState) -> Option<crate::domain::agents::ids::AgentId> {
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
        || agent.cwd.to_string_lossy().to_lowercase().contains(filter)
        || agent
            .branch
            .as_deref()
            .map(|branch| branch.to_lowercase().contains(filter))
            .unwrap_or(false)
}
