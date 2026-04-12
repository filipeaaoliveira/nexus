use std::path::PathBuf;

use chrono::{DateTime, Local};

use crate::domain::accounts::ProviderFilter;
use crate::domain::agents::ids::AgentId;
use crate::domain::agents::status::{NormalizedState, PrimaryStatus};

use super::effect::EffectResult;
use super::state::{
    AccountsConfirmAction, AccountsMode, Overlay, OverviewMode, ProfileDraft, Workflow, Workspace,
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
    CycleSortMode,
    StartFiltering,
    UpdateFilter(String),
    ConfirmFilter,
    ClearFilter,
    StartRenaming { agent_id: AgentId },
    UpdateRenameInput(String),
    DismissExited,
    DismissSelected,
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
    Discovered {
        id: AgentId,
        pid: Option<u32>,
        cwd: PathBuf,
        started_at: Option<DateTime<Local>>,
    },
    SessionInfo {
        id: AgentId,
        pid: Option<u32>,
        session_id: String,
        branch: Option<String>,
        is_worktree: bool,
        started_at: Option<DateTime<Local>>,
    },
    StatusChanged {
        id: AgentId,
        pid: Option<u32>,
        old: PrimaryStatus,
        new: PrimaryStatus,
    },
    NormalizedStateChanged {
        id: AgentId,
        pid: Option<u32>,
        state: NormalizedState,
    },
    ProcessExited {
        id: AgentId,
        pid: Option<u32>,
    },
}
