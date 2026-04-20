use crate::app::action::SystemAction;
use crate::app::effect::Effect;
use crate::app::state::{Overlay, SystemState};

pub fn reduce(state: &mut SystemState, action: &SystemAction) -> Vec<Effect> {
    match action {
        SystemAction::ShowHelp => state.overlay = Overlay::Help { scroll: 0 },
        SystemAction::ScrollHelpUp => {
            if let Overlay::Help { scroll } = &mut state.overlay {
                *scroll = scroll.saturating_sub(1);
            }
        }
        SystemAction::ScrollHelpDown => {
            if let Overlay::Help { scroll } = &mut state.overlay {
                *scroll = scroll.saturating_add(1);
            }
        }
        SystemAction::ClearOverlay => state.overlay = Overlay::None,
        SystemAction::SetWorkflow(workflow) => state.workflow = workflow.clone(),
    }

    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::state::Workflow;

    #[test]
    fn system_reducer_controls_overlay_and_workflow_state() {
        let mut state = SystemState::default();

        reduce(&mut state, &SystemAction::ShowHelp);
        assert_eq!(state.overlay, Overlay::Help { scroll: 0 });

        reduce(&mut state, &SystemAction::ScrollHelpDown);
        assert_eq!(state.overlay, Overlay::Help { scroll: 1 });

        reduce(&mut state, &SystemAction::ScrollHelpUp);
        assert_eq!(state.overlay, Overlay::Help { scroll: 0 });

        reduce(
            &mut state,
            &SystemAction::SetWorkflow(Workflow::LoadingProfiles),
        );
        assert_eq!(state.workflow, Workflow::LoadingProfiles);

        reduce(&mut state, &SystemAction::ClearOverlay);
        assert_eq!(state.overlay, Overlay::None);
    }
}
