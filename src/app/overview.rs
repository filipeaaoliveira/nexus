use super::*;
use crate::app::action::OverviewAction;
use crate::app::state::OverviewMode;

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
        match action {
            OverviewAction::SelectNext => self.select_next_visible(),
            OverviewAction::SelectPrevious => self.select_previous_visible(),
            OverviewAction::CycleSortMode => {
                self.root.overview.sort_mode = self.root.overview.sort_mode.next();
                self.clamp_selection();
            }
            OverviewAction::StartFiltering => {
                self.root.overview.mode = OverviewMode::Filtering {
                    input: String::new(),
                };
                self.root.overview.filter = None;
                self.clamp_selection();
            }
            OverviewAction::UpdateFilter(input) => {
                self.root.overview.mode = OverviewMode::Filtering {
                    input: input.clone(),
                };
                self.root.overview.filter = if input.is_empty() {
                    None
                } else {
                    Some(input)
                };
                self.clamp_selection();
            }
            OverviewAction::ConfirmFilter => {
                self.root.overview.mode = OverviewMode::Normal;
                self.clamp_selection();
            }
            #[cfg(test)]
            OverviewAction::ClearFilter => {
                self.root.overview.mode = OverviewMode::Normal;
                self.root.overview.filter = None;
                self.clamp_selection();
            }
            OverviewAction::StartRenaming { agent_id } => {
                if self
                    .visible_agents()
                    .iter()
                    .any(|agent| agent.id == agent_id)
                {
                    self.root.overview.mode = OverviewMode::Renaming {
                        agent_id,
                        input: String::new(),
                    };
                }
            }
            OverviewAction::UpdateRenameInput(input) => {
                if let OverviewMode::Renaming { agent_id, .. } = &self.root.overview.mode {
                    self.root.overview.mode = OverviewMode::Renaming {
                        agent_id: agent_id.clone(),
                        input,
                    };
                }
            }
            OverviewAction::DismissExited => {
                self.agents.retain(|agent| agent.status != PrimaryStatus::Exited);
                self.clamp_selection();
            }
            OverviewAction::DismissSelected => {
                if let Some(selected_id) = self.selected_visible_agent_id() {
                    self.agents.retain(|agent| agent.id != selected_id);
                    self.clamp_selection();
                }
            }
            OverviewAction::ConfirmRename => {
                if let OverviewMode::Renaming { agent_id, input } = &self.root.overview.mode {
                    if !input.is_empty() {
                        if let Some(agent) = self.agents.iter_mut().find(|agent| agent.id == *agent_id)
                        {
                            agent.alias = Some(input.clone());
                        }
                    }
                }
                self.root.overview.mode = OverviewMode::Normal;
                self.clamp_selection();
            }
            OverviewAction::CancelMode => {
                if matches!(self.root.overview.mode, OverviewMode::Filtering { .. }) {
                    self.root.overview.filter = None;
                }
                self.root.overview.mode = OverviewMode::Normal;
                self.clamp_selection();
            }
        }
    }

    fn selected_visible_agent_id(&self) -> Option<AgentId> {
        crate::app::projection::selected_agent_id(&self.agents, self.root.overview())
    }

    fn select_next_visible(&mut self) {
        let len = self.visible_agents().len();
        if len == 0 {
            return;
        }

        self.root.overview.selected_index = Some(match self.root.overview.selected_index {
            Some(idx) => (idx + 1) % len,
            None => 0,
        });
    }

    fn select_previous_visible(&mut self) {
        let len = self.visible_agents().len();
        if len == 0 {
            return;
        }

        self.root.overview.selected_index = Some(match self.root.overview.selected_index {
            Some(idx) => (idx + len - 1) % len,
            None => len - 1,
        });
    }
}
