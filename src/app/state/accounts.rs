use std::cmp::Ordering;
use std::collections::BTreeMap;

use crate::app::effect::AccountProfile;
use crate::domain::accounts::QuotaSnapshot;
use crate::domain::accounts::{AccountEntry, Provider, ProviderFilter};

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
pub struct AccountKey {
    pub provider: Provider,
    pub profile_id: String,
}

impl AccountKey {
    pub fn new(provider: Provider, profile_id: impl Into<String>) -> Self {
        Self {
            provider,
            profile_id: profile_id.into(),
        }
    }

    pub fn for_entry(entry: &AccountEntry) -> Self {
        Self::new(entry.provider, entry.profile_id.clone())
    }

    pub fn for_profile(profile: &AccountProfile) -> Self {
        Self::new(profile.provider(), profile.profile_id())
    }
}

impl Ord for AccountKey {
    fn cmp(&self, other: &Self) -> Ordering {
        provider_rank(self.provider)
            .cmp(&provider_rank(other.provider))
            .then_with(|| self.profile_id.cmp(&other.profile_id))
    }
}

impl PartialOrd for AccountKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

const fn provider_rank(provider: Provider) -> u8 {
    match provider {
        Provider::Codex => 0,
        Provider::Claude => 1,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountsState {
    pub(in crate::app) mode: AccountsMode,
    pub(in crate::app) selected_index: Option<usize>,
    pub(in crate::app) entries: Vec<AccountEntry>,
    pub(in crate::app) profiles_by_key: BTreeMap<AccountKey, AccountProfile>,
    pub(in crate::app) quota_by_key: BTreeMap<AccountKey, QuotaSnapshot>,
    pub(in crate::app) quota_loading: bool,
    pub(in crate::app) provider_filter: ProviderFilter,
    pub(in crate::app) status_message: Option<String>,
    pub(in crate::app) active_codex_profile_id: Option<String>,
    pub(in crate::app) active_claude_profile_id: Option<String>,
}

impl Default for AccountsState {
    fn default() -> Self {
        Self {
            mode: AccountsMode::Normal,
            selected_index: None,
            entries: Vec::new(),
            profiles_by_key: BTreeMap::new(),
            quota_by_key: BTreeMap::new(),
            quota_loading: false,
            provider_filter: ProviderFilter::All,
            status_message: None,
            active_codex_profile_id: None,
            active_claude_profile_id: None,
        }
    }
}

impl AccountsState {
    pub fn mode(&self) -> &AccountsMode {
        &self.mode
    }

    pub fn selected_index(&self) -> Option<usize> {
        self.selected_index
    }

    pub fn provider_filter(&self) -> ProviderFilter {
        self.provider_filter
    }

    pub fn status_message(&self) -> Option<&str> {
        self.status_message.as_deref()
    }

    pub fn quota_by_key(&self) -> &BTreeMap<AccountKey, QuotaSnapshot> {
        &self.quota_by_key
    }

    pub fn active_codex_profile_id(&self) -> Option<&str> {
        self.active_codex_profile_id.as_deref()
    }

    pub fn active_claude_profile_id(&self) -> Option<&str> {
        self.active_claude_profile_id.as_deref()
    }

    pub fn visible_entry_indices(&self) -> Vec<usize> {
        self.entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| match self.provider_filter {
                ProviderFilter::All => true,
                ProviderFilter::Codex => entry.provider == Provider::Codex,
                ProviderFilter::Claude => entry.provider == Provider::Claude,
            })
            .map(|(idx, _)| idx)
            .collect()
    }

    pub fn visible_entries(&self) -> Vec<&AccountEntry> {
        self.visible_entry_indices()
            .into_iter()
            .map(|idx| &self.entries[idx])
            .collect()
    }

    pub fn selected_entry(&self) -> Option<&AccountEntry> {
        let visible_idx = self.selected_index?;
        let real_idx = *self.visible_entry_indices().get(visible_idx)?;
        self.entries.get(real_idx)
    }

    pub fn clamp_selection(&mut self) {
        let len = self.visible_entry_indices().len();
        self.selected_index = match (self.selected_index, len) {
            (_, 0) => None,
            (Some(idx), len) if idx < len => Some(idx),
            _ => Some(0),
        };
    }

    pub(crate) fn upsert_quota(&mut self, key: AccountKey, quota: QuotaSnapshot) {
        self.quota_by_key.insert(key, quota);
    }

    pub(crate) fn clear_status_message(&mut self) {
        self.status_message = None;
    }

    pub(crate) fn set_quota_loading(&mut self, loading: bool) {
        self.quota_loading = loading;
    }
}
