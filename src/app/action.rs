use chrono::{DateTime, Local};
use nexus_domain::accounts::ProviderFilter;
use nexus_domain::agents::ids::AgentId;

use crate::event::AgentEvent;

use super::effect::EffectResult;
use super::state::{AccountsConfirmAction, AccountsMode};
use super::state::{EditableProfile, Workflow};

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
        now: DateTime<Local>,
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
    /// Used in tests to set up editing state directly. Kept unconditional
    /// so the binary shape is identical between test and production builds.
    #[allow(dead_code)]
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
