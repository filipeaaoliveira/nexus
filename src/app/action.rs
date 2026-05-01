use chrono::{DateTime, Utc};
use nexus_domain::accounts::ProviderFilter;
use nexus_domain::agents::ids::AgentId;

use crate::event::AgentEvent;

use super::effect::EffectResult;
#[cfg(test)]
use super::state::EditableProfile;
use super::state::Workflow;
use super::state::{AccountsConfirmAction, AccountsMode};

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    QuitRequested,
    Workspace(WorkspaceAction),
    Overview(OverviewAction),
    Accounts(AccountsAction),
    System(SystemAction),
    EffectResult(EffectResult),
    AgentEvent {
        event: Box<AgentEvent>,
        now: DateTime<Utc>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceAction {
    Next,
    Previous,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OverviewAction {
    SelectNext,
    SelectPrevious,
    CycleSortMode,
    StartFiltering,
    UpdateFilter(String),
    ConfirmFilter,
    ClearFilter,
    StartRenaming { agent_id: AgentId },
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
    StartEditing(EditableProfile),
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
    SetWorkflow(Workflow),
}
