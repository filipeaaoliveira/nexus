use crate::domain::accounts::ProviderFilter;
use crate::types::AgentId;

use super::effect::EffectResult;
use super::state::{
    AccountsConfirmAction, AccountsMode, Overlay, OverviewMode, ProfileDraft, Workflow,
    Workspace,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Tick,
    QuitRequested,
    Workspace(WorkspaceAction),
    Overview(OverviewAction),
    Accounts(AccountsAction),
    System(SystemAction),
    Detector(DetectorAction),
    EffectResult(EffectResult),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceAction {
    SwitchTo(Workspace),
    Next,
    Previous,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OverviewAction {
    SelectNext,
    SelectPrevious,
    StartFiltering,
    UpdateFilter(String),
    StartRenaming {
        agent_id: AgentId,
    },
    UpdateRenameInput(String),
    SetMode(OverviewMode),
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
    SetOverlay(Overlay),
    ClearOverlay,
    SetWorkflow(Workflow),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DetectorAction {
    HealthChanged {
        detector: String,
        healthy: bool,
        message: String,
    },
}
