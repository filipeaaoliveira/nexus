use crate::app::action::{Action, DetectorAction, WorkspaceAction};
use crate::app::effect::Effect;
use crate::app::reducer::{accounts, overview, system};
use crate::app::state::{RootState, Workflow};

pub fn reduce(state: &mut RootState, action: &Action) -> Vec<Effect> {
    match action {
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
                WorkspaceAction::Next => state.next_workspace(),
            }
            Vec::new()
        }
        Action::Overview(action) => overview::reduce(&mut state.overview, action),
        Action::System(action) => system::reduce(&mut state.system, action),
        Action::Detector(DetectorAction::HealthChanged {
            detector,
            healthy,
            message,
        }) => {
            state.record_detector_health(*detector, *healthy, message.clone());
            Vec::new()
        }
        Action::Detector(_) => Vec::new(),
    }
}
