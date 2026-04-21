use super::*;
use crate::app::reducer;

impl AppState {
    pub fn toggle_help(&mut self) {
        if self.show_help() {
            self.run_system_reducer_action(SystemAction::ClearOverlay);
        } else {
            self.run_system_reducer_action(SystemAction::ShowHelp);
        }
    }

    #[allow(dead_code)]
    pub fn scroll_help_down(&mut self) {
        self.run_system_reducer_action(SystemAction::ScrollHelpDown);
    }

    pub fn dismiss_overlay(&mut self) {
        self.run_system_reducer_action(SystemAction::ClearOverlay);
        self.run_overview_reducer_action(OverviewAction::CancelMode);
        self.clamp_selection();
    }

    pub fn run_workspace_reducer_action(&mut self, action: WorkspaceAction) {
        let effects = reducer::root::reduce(self, &Action::Workspace(action));
        debug_assert!(
            effects.is_empty(),
            "workspace reducer should not emit effects yet"
        );
    }

    pub fn run_system_reducer_action(&mut self, action: SystemAction) {
        let effects = reducer::root::reduce(self, &Action::System(action));
        debug_assert!(
            effects.is_empty(),
            "system reducer should not emit effects yet"
        );
        self.clamp_help_scroll();
    }

    pub(crate) fn clamp_help_scroll(&mut self) {
        let max = crate::ui::help::help_lines(true).len() as u16;
        if let Overlay::Help { scroll } = &mut self.root.system.overlay {
            *scroll = (*scroll).min(max);
        }
    }
}
