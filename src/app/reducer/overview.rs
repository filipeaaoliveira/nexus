use crate::app::action::OverviewAction;
use crate::app::effect::Effect;
use crate::app::state::{OverviewMode, OverviewState};

pub fn reduce(state: &mut OverviewState, action: &OverviewAction) -> Vec<Effect> {
    match action {
        OverviewAction::SelectNext => {
            state.selected_index = Some(match state.selected_index {
                Some(idx) => idx + 1,
                None => 0,
            });
        }
        OverviewAction::SelectPrevious => {
            state.selected_index = Some(match state.selected_index {
                Some(0) | None => 0,
                Some(idx) => idx.saturating_sub(1),
            });
        }
        OverviewAction::CycleSortMode => {
            state.sort_mode = state.sort_mode.next();
        }
        OverviewAction::StartFiltering => {
            state.mode = OverviewMode::Filtering {
                input: String::new(),
            };
            state.filter = None;
        }
        OverviewAction::UpdateFilter(input) => {
            state.mode = OverviewMode::Filtering {
                input: input.clone(),
            };
            state.filter = if input.is_empty() {
                None
            } else {
                Some(input.clone())
            };
        }
        OverviewAction::ConfirmFilter => state.mode = OverviewMode::Normal,
        #[cfg(test)]
        OverviewAction::ClearFilter => {
            state.mode = OverviewMode::Normal;
            state.filter = None;
        }
        OverviewAction::StartRenaming { agent_id } => {
            state.mode = OverviewMode::Renaming {
                agent_id: agent_id.clone(),
                input: String::new(),
            };
        }
        OverviewAction::UpdateRenameInput(input) => {
            if let OverviewMode::Renaming { agent_id, .. } = &state.mode {
                state.mode = OverviewMode::Renaming {
                    agent_id: agent_id.clone(),
                    input: input.clone(),
                };
            }
        }
        OverviewAction::DismissExited
        | OverviewAction::DismissSelected
        | OverviewAction::ConfirmRename
        | OverviewAction::CancelMode => {
            let was_filtering = matches!(state.mode, OverviewMode::Filtering { .. });
            if matches!(
                action,
                OverviewAction::ConfirmRename | OverviewAction::CancelMode
            ) {
                state.mode = OverviewMode::Normal;
            }
            if matches!(action, OverviewAction::CancelMode) && was_filtering {
                state.filter = None;
            }
        }
    }

    Vec::new()
}
