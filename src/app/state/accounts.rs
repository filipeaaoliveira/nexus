use std::cmp::Ordering;
use std::collections::BTreeMap;

use nexus_domain::accounts::{
    AccountEntry, DomainProfile, ProfileId, Provider, ProviderFilter, QuotaSnapshot,
};

/// A plain value object representing editable fields of an account profile.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EditableProfile {
    pub label: String,
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
        draft: EditableProfile,
        field: AccountsEditField,
    },
    Confirming {
        action: AccountsConfirmAction,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountKey {
    pub provider: Provider,
    pub profile_id: ProfileId,
}

impl AccountKey {
    pub fn new(provider: Provider, profile_id: ProfileId) -> Self {
        Self {
            provider,
            profile_id,
        }
    }

    pub fn for_entry(entry: &AccountEntry) -> Self {
        Self::new(entry.provider, ProfileId::new_unchecked(entry.profile_id.as_str()))
    }

    pub fn for_profile(profile: &DomainProfile) -> Self {
        Self::new(profile.provider, ProfileId::new_unchecked(profile.profile_id()))
    }
}

impl Ord for AccountKey {
    fn cmp(&self, other: &Self) -> Ordering {
        provider_rank(self.provider)
            .cmp(&provider_rank(other.provider))
            .then_with(|| self.profile_id.as_str().cmp(other.profile_id.as_str()))
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
        _ => u8::MAX,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountsState {
    pub(in crate::app) mode: AccountsMode,
    pub(in crate::app) selected_index: Option<usize>,
    pub(in crate::app) entries: Vec<AccountEntry>,
    pub(in crate::app) profiles_by_key: BTreeMap<AccountKey, DomainProfile>,
    pub(in crate::app) quota_by_key: BTreeMap<AccountKey, QuotaSnapshot>,
    pub(in crate::app) quota_loading: bool,
    pub(in crate::app) provider_filter: ProviderFilter,
    pub(in crate::app) status_message: Option<String>,
    pub(in crate::app) active_codex_profile_id: Option<ProfileId>,
    pub(in crate::app) active_claude_profile_id: Option<ProfileId>,
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

    pub fn entries(&self) -> &[AccountEntry] {
        &self.entries
    }

    pub fn quota_loading(&self) -> bool {
        self.quota_loading
    }

    pub fn status_message(&self) -> Option<&str> {
        self.status_message.as_deref()
    }

    pub fn profile_by_key(&self, key: &AccountKey) -> Option<&DomainProfile> {
        self.profiles_by_key.get(key)
    }

    pub fn quota_by_key(&self) -> &BTreeMap<AccountKey, QuotaSnapshot> {
        &self.quota_by_key
    }

    pub fn active_codex_profile_id(&self) -> Option<&ProfileId> {
        self.active_codex_profile_id.as_ref()
    }

    pub fn active_claude_profile_id(&self) -> Option<&ProfileId> {
        self.active_claude_profile_id.as_ref()
    }

    pub fn visible_entry_indices(&self) -> Vec<usize> {
        self.entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| match self.provider_filter {
                ProviderFilter::All => true,
                ProviderFilter::Codex => entry.provider == Provider::Codex,
                ProviderFilter::Claude => entry.provider == Provider::Claude,
                _ => true,
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
