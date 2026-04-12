use crate::app::action::{Action, DetectorAction, SystemAction, WorkspaceAction};
use crate::app::effect::Effect;
use crate::app::reducer::{accounts, overview};
use crate::app::state::{Overlay, RootState, Workflow};

pub fn reduce(state: &mut RootState, action: &Action) -> Vec<Effect> {
    match action {
        Action::Tick => Vec::new(),
        Action::EffectResult(result) => {
            accounts::reduce_result(&mut state.accounts, &mut state.system.workflow, result);
            Vec::new()
        }
        Action::Accounts(action) => {
            accounts::reduce(&mut state.accounts, &mut state.system.workflow, action)
        }
        Action::QuitRequested => {
            state.system.workflow = Workflow::Quitting;
            Vec::new()
        }
        Action::Workspace(action) => {
            match action {
                WorkspaceAction::SwitchTo(workspace) => state.switch_workspace(*workspace),
                WorkspaceAction::Next => state.next_workspace(),
                WorkspaceAction::Previous => state.previous_workspace(),
            }
            Vec::new()
        }
        Action::Overview(action) => overview::reduce(&mut state.overview, action),
        Action::System(action) => {
            match action {
                SystemAction::ShowHelp => state.show_overlay(Overlay::Help { scroll: 0 }),
                SystemAction::ScrollHelpUp => {
                    if let Overlay::Help { scroll } = &mut state.system.overlay {
                        *scroll = scroll.saturating_sub(1);
                    }
                }
                SystemAction::ScrollHelpDown => {
                    if let Overlay::Help { scroll } = &mut state.system.overlay {
                        *scroll = scroll.saturating_add(1);
                    }
                }
                SystemAction::SetOverlay(overlay) => state.show_overlay(overlay.clone()),
                SystemAction::ClearOverlay => state.clear_overlay(),
                SystemAction::SetWorkflow(workflow) => state.system.workflow = workflow.clone(),
            }
            Vec::new()
        }
        Action::Detector(action) => match action {
            DetectorAction::HealthChanged {
                detector,
                healthy,
                message,
            } => {
                state.record_detector_health(detector.clone(), *healthy, message.clone());
                Vec::new()
            }
            _ => overview::reduce_detector(&mut state.overview, action),
        },
    }
}
