use super::*;
use nexus_domain::accounts::ProfileId;

impl AppState {
    pub(crate) fn set_accounts_quota_loading(&mut self, loading: bool) {
        self.root.accounts.set_quota_loading(loading);
    }

    pub(crate) fn clear_accounts_status_message(&mut self) {
        self.root.accounts.clear_status_message();
    }

    pub(crate) fn accounts_status_message(&self) -> Option<&str> {
        self.root.accounts.status_message()
    }

    pub fn dismiss_accounts_overlay(&mut self) {
        self.root.clear_overlay();
        self.root.accounts.mode = AccountsMode::Normal;
    }

    pub fn selected_account(&self) -> Option<(Provider, &str)> {
        self.root
            .accounts
            .selected_entry()
            .map(|entry| (entry.provider, entry.profile_id.as_str()))
    }

    #[allow(dead_code)]
    pub fn set_active_codex_profile(&mut self, profile_id: Option<ProfileId>) {
        self.root.accounts.active_codex_profile_id = profile_id;
    }

    #[allow(dead_code)]
    pub fn active_codex_profile_id(&self) -> Option<&ProfileId> {
        self.root.accounts.active_codex_profile_id.as_ref()
    }
}
