use super::*;
use crate::app::action::{Action, OverviewAction};
use crate::app::reducer;

impl AppState {
    #[cfg(test)]
    pub fn move_selection_down(&mut self) {
        self.run_overview_reducer_action(OverviewAction::SelectNext);
    }

    #[cfg(test)]
    pub fn move_selection_up(&mut self) {
        self.run_overview_reducer_action(OverviewAction::SelectPrevious);
    }

    #[cfg(test)]
    pub fn cycle_sort_mode(&mut self) {
        self.run_overview_reducer_action(OverviewAction::CycleSortMode);
    }

    #[cfg(test)]
    pub fn start_filter(&mut self) {
        self.run_overview_reducer_action(OverviewAction::StartFiltering);
    }

    #[cfg(test)]
    pub fn filter_push_char(&mut self, c: char) {
        let mut input = match &self.root.overview.mode {
            OverviewMode::Filtering { input } => input.clone(),
            _ => String::new(),
        };
        input.push(c);
        self.run_overview_reducer_action(OverviewAction::UpdateFilter(input));
    }

    #[cfg(test)]
    pub fn confirm_filter(&mut self) {
        self.run_overview_reducer_action(OverviewAction::ConfirmFilter);
    }

    pub fn start_rename(&mut self) {
        let Some(agent_id) = self.selected_visible_agent_id() else {
            return;
        };
        self.run_overview_reducer_action(OverviewAction::StartRenaming { agent_id });
    }

    #[cfg(test)]
    pub fn confirm_rename(&mut self) {
        self.run_overview_reducer_action(OverviewAction::ConfirmRename);
    }

    #[cfg(test)]
    pub fn cancel_rename(&mut self) {
        self.run_overview_reducer_action(OverviewAction::CancelMode);
    }

    pub fn remove_exited(&mut self) {
        self.run_overview_reducer_action(OverviewAction::DismissExited);
    }

    pub fn remove_selected(&mut self) {
        self.run_overview_reducer_action(OverviewAction::DismissSelected);
    }

    pub(super) fn clamp_selection(&mut self) {
        let visible_count = self.visible_agents().len();
        self.root.overview.clamp_selection(visible_count);
    }

    pub(super) fn mark_agents_dirty(&self) {}

    pub fn run_overview_reducer_action(&mut self, action: OverviewAction) {
        reducer::root::reduce(self, &Action::Overview(action));
    }

    pub(super) fn selected_visible_agent_id(&self) -> Option<AgentId> {
        crate::app::projection::selected_agent_id(&self.agents, self.root.overview())
    }
}
