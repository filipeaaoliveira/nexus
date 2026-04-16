use super::*;
#[cfg(test)]
use crate::domain::accounts::ProfileId;

impl AppState {
    pub(crate) fn accounts_root_state(&self) -> RootState {
        self.root.clone()
    }

    pub(crate) fn apply_accounts_root_state(&mut self, root: &RootState) {
        self.root.accounts = root.accounts.clone();
        self.root.system = root.system.clone();
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

    #[cfg(test)]
    pub fn set_active_codex_profile(&mut self, profile_id: Option<ProfileId>) {
        self.root.accounts.active_codex_profile_id = profile_id;
    }

    #[cfg(test)]
    pub fn active_codex_profile_id(&self) -> Option<&ProfileId> {
        self.root.accounts.active_codex_profile_id.as_ref()
    }
}
