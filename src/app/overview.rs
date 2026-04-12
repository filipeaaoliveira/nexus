use super::*;
use crate::app::action::OverviewAction;
use crate::app::reducer;
use crate::app::state::{OverviewAgent, OverviewMode};

impl AppState {
    pub fn move_selection_down(&mut self) {
        self.run_overview_reducer_action(OverviewAction::SelectNext);
    }

    pub fn move_selection_up(&mut self) {
        self.run_overview_reducer_action(OverviewAction::SelectPrevious);
    }

    pub fn cycle_sort_mode(&mut self) {
        self.run_overview_reducer_action(OverviewAction::CycleSortMode);
    }

    pub fn toggle_help(&mut self) {
        if self.show_help() {
            self.root.clear_overlay();
        } else {
            self.root.show_overlay(Overlay::Help { scroll: 0 });
        }
    }

    pub fn scroll_help_up(&mut self) {
        if let Overlay::Help { scroll } = &mut self.root.system.overlay {
            *scroll = scroll.saturating_sub(1);
        }
    }

    pub fn scroll_help_down(&mut self) {
        let max = crate::ui::help::help_lines(true).len() as u16;
        if let Overlay::Help { scroll } = &mut self.root.system.overlay {
            *scroll = scroll.saturating_add(1).min(max);
        }
    }

    pub fn dismiss_overlay(&mut self) {
        self.root.clear_overlay();
        self.run_overview_reducer_action(OverviewAction::CancelMode);
        self.clamp_selection();
    }

    pub fn start_filter(&mut self) {
        self.run_overview_reducer_action(OverviewAction::StartFiltering);
    }

    pub fn filter_push_char(&mut self, c: char) {
        let mut input = match &self.root.overview.mode {
            OverviewMode::Filtering { input } => input.clone(),
            _ => String::new(),
        };
        input.push(c);
        self.run_overview_reducer_action(OverviewAction::UpdateFilter(input));
    }

    pub fn filter_pop_char(&mut self) {
        let mut input = match &self.root.overview.mode {
            OverviewMode::Filtering { input } => input.clone(),
            _ => String::new(),
        };
        input.pop();
        self.run_overview_reducer_action(OverviewAction::UpdateFilter(input));
    }

    pub fn confirm_filter(&mut self) {
        self.run_overview_reducer_action(OverviewAction::ConfirmFilter);
        self.clamp_selection();
    }

    pub fn start_rename(&mut self) {
        let Some(agent_id) = self.root.overview.selected_agent_id() else {
            return;
        };
        self.run_overview_reducer_action(OverviewAction::StartRenaming { agent_id });
    }

    pub fn rename_push_char(&mut self, c: char) {
        let mut input = match &self.root.overview.mode {
            OverviewMode::Renaming { input, .. } => input.clone(),
            _ => String::new(),
        };
        input.push(c);
        self.run_overview_reducer_action(OverviewAction::UpdateRenameInput(input));
    }

    pub fn rename_pop_char(&mut self) {
        let mut input = match &self.root.overview.mode {
            OverviewMode::Renaming { input, .. } => input.clone(),
            _ => String::new(),
        };
        input.pop();
        self.run_overview_reducer_action(OverviewAction::UpdateRenameInput(input));
    }

    pub fn confirm_rename(&mut self) {
        self.run_overview_reducer_action(OverviewAction::ConfirmRename);
        self.sync_agent_aliases_from_root_overview();
    }

    pub fn cancel_rename(&mut self) {
        self.run_overview_reducer_action(OverviewAction::CancelMode);
    }

    pub fn remove_exited(&mut self) {
        self.run_overview_reducer_action(OverviewAction::DismissExited);
        self.retain_agents_present_in_root_overview();
        self.clamp_selection();
    }

    pub fn remove_selected(&mut self) {
        self.run_overview_reducer_action(OverviewAction::DismissSelected);
        self.retain_agents_present_in_root_overview();
        self.clamp_selection();
    }

    pub(super) fn clamp_selection(&mut self) {
        self.root.overview.clamp_selection();
    }

    pub(super) fn mark_agents_dirty(&self) {
        self.agents_version.set(self.agents_version.get() + 1);
    }

    pub(super) fn with_visible_agent_indices<R>(&self, f: impl FnOnce(&[usize]) -> R) -> R {
        self.refresh_visible_agent_cache();
        let cache = self.visible_agent_cache.borrow();
        f(&cache.indices)
    }

    pub fn run_overview_reducer_action(&mut self, action: OverviewAction) {
        let effects = reducer::root::reduce(&mut self.root, &crate::app::Action::Overview(action));
        debug_assert!(effects.is_empty(), "overview reducer should not emit effects yet");
    }

    pub fn sync_root_overview_agents(&mut self) {
        self.root.overview.agents = self
            .agents
            .iter()
            .map(|agent| OverviewAgent {
                id: agent.id.clone(),
                pid: agent.pid,
                status: agent.status,
                normalized: agent.normalized.clone(),
                cwd: agent.cwd.clone(),
                branch: agent.branch.clone(),
                is_worktree: agent.is_worktree,
                started_at: Some(agent.started_at),
                session_id: agent.session_id.clone(),
                alias: agent.alias.clone(),
            })
            .collect();
    }

    fn sync_agent_aliases_from_root_overview(&mut self) {
        for root_agent in &self.root.overview.agents {
            if let Some(agent) = self.agents.iter_mut().find(|agent| agent.id == root_agent.id) {
                agent.alias = root_agent.alias.clone();
            }
        }
        self.mark_agents_dirty();
    }

    fn retain_agents_present_in_root_overview(&mut self) {
        let retained_ids: Vec<_> = self
            .root
            .overview
            .agents
            .iter()
            .map(|agent| agent.id.clone())
            .collect();
        self.agents
            .retain(|agent| retained_ids.iter().any(|id| id == &agent.id));
        self.mark_agents_dirty();
    }

    fn refresh_visible_agent_cache(&self) {
        let current_version = self.agents_version.get();
        let current_filter = self.root.overview.filter.clone();
        let current_sort_mode = self.root.overview.sort_mode;
        let should_refresh = {
            let cache = self.visible_agent_cache.borrow();
            cache.version != current_version
                || cache.filter != current_filter
                || cache.sort_mode != Some(current_sort_mode)
        };

        if !should_refresh {
            return;
        }

        let indices = Self::compute_visible_agent_indices(
            &self.agents,
            current_filter.as_deref(),
            current_sort_mode,
        );
        *self.visible_agent_cache.borrow_mut() = VisibleAgentCache {
            version: current_version,
            filter: current_filter,
            sort_mode: Some(current_sort_mode),
            indices,
        };
    }

    fn compute_visible_agent_indices(
        agents: &[Agent],
        filter: Option<&str>,
        sort_mode: SortMode,
    ) -> Vec<usize> {
        let mut indices: Vec<usize> = agents
            .iter()
            .enumerate()
            .filter(|(_, agent)| {
                !(agent.id.tool() == AgentTool::Codex
                    && agent.id.is_session()
                    && agent.pid.is_none())
            })
            .map(|(idx, _)| idx)
            .collect();

        if let Some(filter) = filter {
            let filter = filter.to_lowercase();
            indices.retain(|&idx| Self::agent_matches_filter(&agents[idx], &filter));
        }

        match sort_mode {
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
                indices.sort_by(|&left, &right| {
                    agents[left].started_at.cmp(&agents[right].started_at)
                });
            }
        }

        indices
    }

    pub(super) fn agent_matches_filter(agent: &Agent, filter: &str) -> bool {
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
}
