use crate::app::action::{Action, WorkspaceAction};
use crate::app::core::AppState;
use crate::app::effect::Effect;
use crate::app::reducer::{accounts, agents, overview, system};
use crate::app::state::Workflow;

pub fn reduce(app: &mut AppState, action: &Action) -> Vec<Effect> {
    match action {
        Action::EffectResult(result) => {
            accounts::reduce_result(
                &mut app.root.accounts,
                &mut app.root.system.workflow,
                result,
            );
            Vec::new()
        }
        Action::Accounts(action) => {
            accounts::reduce(&mut app.root.accounts, &mut app.root.system.workflow, action)
        }
        Action::QuitRequested => {
            app.root.system.workflow = Workflow::Quitting;
            Vec::new()
        }
        Action::Workspace(action) => {
            match action {
                WorkspaceAction::Next => app.root.next_workspace(),
                WorkspaceAction::Previous => app.root.previous_workspace(),
            }
            Vec::new()
        }
        Action::Overview(action) => overview::reduce(&mut app.agents, &mut app.root.overview, action),
        Action::System(action) => system::reduce(&mut app.root.system, action),
        Action::AgentEvent { event, now } => {
            agents::reduce(&mut app.agents, &mut app.root.overview, &mut app.root.system, event, *now);
            Vec::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::action::{Action, SystemAction};
    use crate::app::core::AppState;
    use crate::app::state::Overlay;

    #[test]
    fn reduce_accepts_app_state_and_delegates_to_sub_reducers() {
        let mut app = AppState::new();
        let effects = reduce(&mut app, &Action::System(SystemAction::ShowHelp));
        assert!(effects.is_empty());
        assert_eq!(*app.root().system().overlay(), Overlay::Help { scroll: 0 });
    }
}
