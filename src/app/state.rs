use std::collections::BTreeMap;

use crate::domain::accounts::{AccountEntry, Provider, ProviderFilter};
use crate::types::{AgentId, SortMode};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Workspace {
    #[default]
    Overview,
    Accounts,
}

impl Workspace {
    pub const ALL: [Self; 2] = [Self::Overview, Self::Accounts];

    pub fn next(self) -> Self {
        match self {
            Self::Overview => Self::Accounts,
            Self::Accounts => Self::Overview,
        }
    }

    pub fn previous(self) -> Self {
        match self {
            Self::Overview => Self::Accounts,
            Self::Accounts => Self::Overview,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Overview => "Overview",
            Self::Accounts => "Accounts",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WorkspaceState {
    pub active: Workspace,
}

impl WorkspaceState {
    pub fn switch_to(&mut self, workspace: Workspace) {
        self.active = workspace;
    }

    pub fn next(&mut self) {
        self.active = self.active.next();
    }

    pub fn previous(&mut self) {
        self.active = self.active.previous();
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum OverviewMode {
    #[default]
    Normal,
    Filtering {
        input: String,
    },
    Renaming {
        agent_id: AgentId,
        input: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OverviewState {
    pub mode: OverviewMode,
    pub selected_index: Option<usize>,
    pub sort_mode: SortMode,
}

impl Default for OverviewState {
    fn default() -> Self {
        Self {
            mode: OverviewMode::Normal,
            selected_index: None,
            sort_mode: SortMode::Name,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ProfileDraft {
    pub label: String,
    pub email: Option<String>,
    pub plan: Option<String>,
    pub notes: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountsConfirmAction {
    DeleteProfile,
    WriteProfileToHome,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AccountsEditField {
    #[default]
    Label,
    Notes,
    Enabled,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum AccountsMode {
    #[default]
    Normal,
    Filtering {
        input: String,
    },
    Editing {
        draft: ProfileDraft,
        field: AccountsEditField,
    },
    Confirming {
        action: AccountsConfirmAction,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountsState {
    pub mode: AccountsMode,
    pub selected_index: Option<usize>,
    pub entries: Vec<AccountEntry>,
    pub provider_filter: ProviderFilter,
    pub status_message: Option<String>,
}

impl Default for AccountsState {
    fn default() -> Self {
        Self {
            mode: AccountsMode::Normal,
            selected_index: None,
            entries: Vec::new(),
            provider_filter: ProviderFilter::All,
            status_message: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetectorHealth {
    pub healthy: bool,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Overlay {
    #[default]
    None,
    Help {
        scroll: u16,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Workflow {
    #[default]
    Idle,
    LoadingProfiles,
    RefreshingQuota {
        provider: Provider,
        profile_id: String,
    },
    ImportingProfile {
        provider: Provider,
    },
    WritingProfileToHome {
        provider: Provider,
        profile_id: String,
    },
    DeletingProfile {
        provider: Provider,
        profile_id: String,
    },
    Quitting,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SystemState {
    pub overlay: Overlay,
    pub workflow: Workflow,
    pub detector_health: BTreeMap<String, DetectorHealth>,
}

impl SystemState {
    pub fn show_overlay(&mut self, overlay: Overlay) {
        self.overlay = overlay;
    }

    pub fn clear_overlay(&mut self) {
        self.overlay = Overlay::None;
    }

    pub fn record_detector_health(
        &mut self,
        detector: impl Into<String>,
        healthy: bool,
        message: impl Into<String>,
    ) {
        self.detector_health.insert(
            detector.into(),
            DetectorHealth {
                healthy,
                message: message.into(),
            },
        );
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RootState {
    pub workspace: WorkspaceState,
    pub overview: OverviewState,
    pub accounts: AccountsState,
    pub system: SystemState,
}

impl RootState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn switch_workspace(&mut self, workspace: Workspace) {
        self.workspace.switch_to(workspace);
    }

    pub fn next_workspace(&mut self) {
        self.workspace.next();
    }

    pub fn previous_workspace(&mut self) {
        self.workspace.previous();
    }

    pub fn show_overlay(&mut self, overlay: Overlay) {
        self.system.show_overlay(overlay);
    }

    pub fn clear_overlay(&mut self) {
        self.system.clear_overlay();
    }

    pub fn record_detector_health(
        &mut self,
        detector: impl Into<String>,
        healthy: bool,
        message: impl Into<String>,
    ) {
        self.system
            .record_detector_health(detector, healthy, message);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::accounts::Provider;

    #[test]
    fn root_state_composes_workspace_overview_accounts_and_system() {
        let state = RootState::default();

        assert_eq!(state.workspace.active, Workspace::Overview);
        assert_eq!(state.overview.mode, OverviewMode::Normal);
        assert_eq!(state.overview.sort_mode, SortMode::Name);
        assert_eq!(state.accounts.mode, AccountsMode::Normal);
        assert_eq!(state.system.overlay, Overlay::None);
        assert_eq!(state.system.workflow, Workflow::Idle);
        assert!(state.system.detector_health.is_empty());
    }

    #[test]
    fn workspace_switching_updates_active_workspace() {
        let mut state = RootState::default();

        state.switch_workspace(Workspace::Accounts);
        assert_eq!(state.workspace.active, Workspace::Accounts);

        state.next_workspace();
        assert_eq!(state.workspace.active, Workspace::Overview);

        state.previous_workspace();
        assert_eq!(state.workspace.active, Workspace::Accounts);
    }

    #[test]
    fn overlay_state_is_explicit() {
        let mut state = RootState::default();

        assert_eq!(state.system.overlay, Overlay::None);

        state.show_overlay(Overlay::Help { scroll: 2 });
        assert_eq!(state.system.overlay, Overlay::Help { scroll: 2 });

        state.clear_overlay();
        assert_eq!(state.system.overlay, Overlay::None);
    }

    #[test]
    fn overview_and_accounts_modes_initialize_to_normal() {
        assert_eq!(OverviewState::default().mode, OverviewMode::Normal);
        assert_eq!(AccountsState::default().mode, AccountsMode::Normal);
    }

    #[test]
    fn workspace_navigation_matches_two_tab_app() {
        assert_eq!(Workspace::ALL, [Workspace::Overview, Workspace::Accounts]);
        assert_eq!(Workspace::Overview.next(), Workspace::Accounts);
        assert_eq!(Workspace::Accounts.next(), Workspace::Overview);
        assert_eq!(Workspace::Overview.previous(), Workspace::Accounts);
        assert_eq!(Workspace::Accounts.previous(), Workspace::Overview);
        assert_eq!(Workspace::Overview.label(), "Overview");
        assert_eq!(Workspace::Accounts.label(), "Accounts");
    }

    #[test]
    fn accounts_state_defaults_to_single_selection_source_of_truth() {
        let state = AccountsState::default();

        assert_eq!(state.selected_index, None);
        assert!(state.entries.is_empty());
        assert_eq!(state.provider_filter, ProviderFilter::All);
        assert_eq!(state.status_message, None);
    }

    #[test]
    fn accounts_edit_mode_uses_semantic_edit_field() {
        let mode = AccountsMode::Editing {
            draft: ProfileDraft::default(),
            field: AccountsEditField::Notes,
        };

        assert_eq!(
            mode,
            AccountsMode::Editing {
                draft: ProfileDraft::default(),
                field: AccountsEditField::Notes,
            }
        );
    }

    #[test]
    fn accounts_entries_provide_selection_target() {
        let mut state = AccountsState::default();
        state.entries.push(AccountEntry {
            provider: Provider::Codex,
            profile_id: "codex-main".to_string(),
        });
        state.selected_index = Some(0);

        assert_eq!(
            state.entries.get(state.selected_index.unwrap()),
            Some(&AccountEntry {
                provider: Provider::Codex,
                profile_id: "codex-main".to_string(),
            })
        );
    }

    #[test]
    fn system_state_stores_detector_health_by_detector() {
        let mut state = RootState::default();

        state.record_detector_health("codex", false, "sqlite unavailable");
        assert_eq!(
            state.system.detector_health.get("codex"),
            Some(&DetectorHealth {
                healthy: false,
                message: "sqlite unavailable".to_string(),
            })
        );

        state.record_detector_health("codex", true, "recovered");
        assert_eq!(
            state.system.detector_health.get("codex"),
            Some(&DetectorHealth {
                healthy: true,
                message: "recovered".to_string(),
            })
        );
    }
}
