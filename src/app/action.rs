use crate::domain::accounts::ProviderFilter;
use crate::domain::agents::ids::AgentId;

use super::effect::EffectResult;
use super::state::{AccountsConfirmAction, AccountsMode};
#[cfg(test)]
use super::state::{ProfileDraft, Workflow};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    QuitRequested,
    Workspace(WorkspaceAction),
    Overview(OverviewAction),
    Accounts(AccountsAction),
    System(SystemAction),
    EffectResult(EffectResult),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceAction {
    Next,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OverviewAction {
    SelectNext,
    SelectPrevious,
    CycleSortMode,
    StartFiltering,
    UpdateFilter(String),
    ConfirmFilter,
    #[cfg(test)]
    ClearFilter,
    StartRenaming {
        agent_id: AgentId,
    },
    UpdateRenameInput(String),
    DismissExited,
    DismissSelected,
    ConfirmRename,
    CancelMode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccountsAction {
    SelectNext,
    SelectPrevious,
    SetProviderFilter(ProviderFilter),
    StartFiltering,
    UpdateFilter(String),
    #[cfg(test)]
    StartEditing(ProfileDraft),
    UpdateEditBuffer(String),
    ToggleEnabled,
    RequestConfirmation(AccountsConfirmAction),
    SetMode(AccountsMode),
    Confirm,
    CancelMode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SystemAction {
    ShowHelp,
    ScrollHelpUp,
    ScrollHelpDown,
    ClearOverlay,
    #[cfg(test)]
    SetWorkflow(Workflow),
}