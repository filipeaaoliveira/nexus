mod accounts;
mod overview;
mod system;
mod workspace;

pub use accounts::{
    AccountKey, AccountsConfirmAction, AccountsEditField, AccountsMode, AccountsState,
    EditableProfile,
};
pub use overview::{OverviewMode, OverviewState};
#[allow(unused_imports)]
pub use system::DetectorHealth;
pub use system::{Overlay, SystemState, Workflow};
pub use workspace::{Workspace, WorkspaceState};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RootState {
    pub(in crate::app) workspace: WorkspaceState,
    pub(in crate::app) overview: OverviewState,
    pub(in crate::app) accounts: AccountsState,
    pub(in crate::app) system: SystemState,
}

impl RootState {
    #[allow(dead_code)]
    pub fn switch_workspace(&mut self, workspace: Workspace) {
        self.workspace.switch_to(workspace);
    }

    pub fn workspace(&self) -> &WorkspaceState {
        &self.workspace
    }

    pub fn overview(&self) -> &OverviewState {
        &self.overview
    }

    pub fn accounts(&self) -> &AccountsState {
        &self.accounts
    }

    pub fn system(&self) -> &SystemState {
        &self.system
    }

    pub fn next_workspace(&mut self) {
        self.workspace.next();
    }

    pub fn previous_workspace(&mut self) {
        self.workspace.previous();
    }

    #[allow(dead_code)]
    pub fn show_overlay(&mut self, overlay: Overlay) {
        self.system.show_overlay(overlay);
    }

    pub fn clear_overlay(&mut self) {
        self.system.clear_overlay();
    }

    pub fn record_detector_health(
        &mut self,
        detector: nexus_domain::system::DetectorKind,
        healthy: bool,
        message: impl Into<String>,
    ) {
        self.system
            .record_detector_health(detector, healthy, message);
    }

    pub(crate) fn load_cached_quota(
        &mut self,
        provider: nexus_domain::accounts::Provider,
        profile_id: impl Into<String>,
        quota: nexus_domain::accounts::QuotaSnapshot,
    ) {
        self.accounts
            .upsert_quota(AccountKey::new(provider, profile_id.into()), quota);
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use super::*;
    use crate::claude_profiles::profile::StoredClaudeProfile;
    use crate::codex_profiles::profile::StoredCodexProfile;
    use nexus_domain::accounts::{
        AccountEntry, Confidence, LimitState, Provider, ProviderFilter, QuotaSnapshot,
    };
    use nexus_domain::agents::model::SortMode;

    fn codex_profile(id: &str) -> StoredCodexProfile {
        let now = Utc::now();
        StoredCodexProfile {
            id: id.to_string(),
            label: format!("Codex {id}"),
            email: Some(format!("{id}@example.com")),
            plan: Some("pro".to_string()),
            notes: Some("notes".to_string()),
            enabled: true,
            last_used: None,
            created_at: now,
            updated_at: now,
        }
    }

    fn claude_profile(id: &str) -> StoredClaudeProfile {
        let now = Utc::now();
        StoredClaudeProfile {
            id: id.to_string(),
            label: format!("Claude {id}"),
            email: Some(format!("{id}@example.com")),
            plan: Some("max".to_string()),
            notes: Some("notes".to_string()),
            enabled: true,
            last_used: None,
            created_at: now,
            updated_at: now,
        }
    }

    fn quota_snapshot(summary: &str) -> QuotaSnapshot {
        QuotaSnapshot::legacy_fallback(
            Utc::now(),
            Confidence::Exact,
            LimitState::Available,
            None,
            Some(42),
            LimitState::Available,
            None,
            Some(84),
            summary.to_string(),
        )
    }

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
        assert!(state.profiles_by_key.is_empty());
        assert!(state.quota_by_key.is_empty());
        assert_eq!(state.provider_filter, ProviderFilter::All);
        assert_eq!(state.status_message, None);
    }

    #[test]
    fn accounts_edit_mode_uses_semantic_edit_field() {
        let mode = AccountsMode::Editing {
            draft: EditableProfile::default(),
            field: AccountsEditField::Notes,
        };

        assert_eq!(
            mode,
            AccountsMode::Editing {
                draft: EditableProfile::default(),
                field: AccountsEditField::Notes,
            }
        );
    }

    #[test]
    fn accounts_entries_provide_selection_target() {
        let mut state = AccountsState::default();
        state.entries.push(AccountEntry {
            provider: Provider::Codex,
            profile_id: "codex-main".into(),
        });
        state.selected_index = Some(0);

        assert_eq!(
            state.entries.get(state.selected_index.unwrap()),
            Some(&AccountEntry {
                provider: Provider::Codex,
                profile_id: "codex-main".into(),
            })
        );
    }

    #[test]
    fn accounts_state_can_store_profiles_and_quota_by_account_key() {
        let mut state = AccountsState::default();
        let codex_key = AccountKey::for_entry(&AccountEntry {
            provider: Provider::Codex,
            profile_id: "codex-main".into(),
        });
        let claude_dp = claude_profile("claude-main").to_domain(Provider::Claude);
        let claude_key = AccountKey::for_profile(&claude_dp);

        state.profiles_by_key.insert(
            codex_key.clone(),
            codex_profile("codex-main").to_domain(Provider::Codex),
        );
        state.profiles_by_key.insert(claude_key.clone(), claude_dp);
        state
            .quota_by_key
            .insert(codex_key.clone(), quota_snapshot("codex fresh"));
        state
            .quota_by_key
            .insert(claude_key.clone(), quota_snapshot("claude fresh"));

        assert_eq!(state.profiles_by_key.len(), 2);
        assert_eq!(state.quota_by_key[&codex_key].summary, "codex fresh");
        assert_eq!(state.quota_by_key[&claude_key].summary, "claude fresh");
    }

    #[test]
    fn account_keys_align_entries_profiles_and_quota_payloads() {
        let entry = AccountEntry {
            provider: Provider::Codex,
            profile_id: "codex-main".into(),
        };
        let profile = codex_profile("codex-main").to_domain(Provider::Codex);

        assert_eq!(
            AccountKey::for_entry(&entry),
            AccountKey::for_profile(&profile)
        );
        assert_ne!(
            AccountKey::for_entry(&entry),
            AccountKey::new(Provider::Claude, "codex-main"),
        );
    }

    #[test]
    fn system_state_stores_detector_health_by_detector() {
        let mut state = RootState::default();

        state.record_detector_health(
            nexus_domain::system::DetectorKind::Codex,
            false,
            "sqlite unavailable",
        );
        assert_eq!(
            state
                .system
                .detector_health
                .get(&nexus_domain::system::DetectorKind::Codex),
            Some(&DetectorHealth {
                healthy: false,
                message: "sqlite unavailable".to_string(),
            })
        );

        state.record_detector_health(nexus_domain::system::DetectorKind::Codex, true, "recovered");
        assert_eq!(
            state
                .system
                .detector_health
                .get(&nexus_domain::system::DetectorKind::Codex),
            Some(&DetectorHealth {
                healthy: true,
                message: "recovered".to_string(),
            })
        );
    }
}
