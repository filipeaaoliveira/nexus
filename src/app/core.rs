use super::*;

pub struct AppState {
    pub(in crate::app) root: RootState,
    pub(in crate::app) agents: Vec<Agent>,
}

#[cfg(test)]
impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

impl AppState {
    #[cfg(test)]
    pub fn new() -> Self {
        Self::from_root(RootState::default())
    }

    pub fn from_root(root: RootState) -> Self {
        Self {
            root,
            agents: Vec::new(),
        }
    }

    pub fn active_tab(&self) -> Workspace {
        self.root.workspace().active()
    }

    pub fn root(&self) -> &RootState {
        &self.root
    }

    pub fn agents(&self) -> &[Agent] {
        &self.agents
    }

    #[allow(dead_code)]
    pub fn next_tab(&mut self) {
        self.run_workspace_reducer_action(super::action::WorkspaceAction::Next);
    }

    #[allow(dead_code)]
    pub fn previous_tab(&mut self) {
        self.run_workspace_reducer_action(super::action::WorkspaceAction::Previous);
    }

    pub fn show_help(&self) -> bool {
        matches!(self.root.system().overlay(), Overlay::Help { .. })
    }

    pub fn help_scroll(&self) -> u16 {
        match self.root.system().overlay() {
            Overlay::Help { scroll } => *scroll,
            Overlay::None => 0,
        }
    }

    pub fn handle_event(&mut self, event: crate::event::AgentEvent) {
        let now = chrono::Local::now();
        super::reducer::root::reduce(self, &super::action::Action::AgentEvent {
            event: Box::new(event),
            now,
        });
    }

    pub fn visible_agents(&self) -> Vec<&Agent> {
        crate::app::projection::visible_agents(&self.agents, self.root.overview())
    }

    #[cfg(test)]
    pub fn count_by_status(&self, status: PrimaryStatus) -> usize {
        crate::app::projection::count_by_status(&self.agents, self.root.overview(), status)
    }

    pub fn primary_detector_issue(&self) -> Option<&str> {
        self.root
            .system()
            .detector_health()
            .values()
            .find(|health| !health.healthy)
            .map(|health| health.message.as_str())
    }
}

#[cfg(test)]
#[path = "core_tests.rs"]
mod tests;
