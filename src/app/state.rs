use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::path::PathBuf;

use chrono::{DateTime, Local};

use super::effect::AccountProfile;
use crate::domain::accounts::QuotaSnapshot;
use crate::domain::accounts::{AccountEntry, Provider, ProviderFilter};
use crate::domain::agents::ids::{AgentId, AgentTool};
use crate::domain::agents::model::SortMode;
use crate::domain::agents::status::{NormalizedState, NormalizedStatus, PrimaryStatus};

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
    pub filter: Option<String>,
    pub agents: Vec<OverviewAgent>,
}

impl Default for OverviewState {
    fn default() -> Self {
        Self {
            mode: OverviewMode::Normal,
            selected_index: None,
            sort_mode: SortMode::Name,
            filter: None,
            agents: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OverviewAgent {
    pub id: AgentId,
    pub pid: Option<u32>,
    pub status: PrimaryStatus,
    pub normalized: NormalizedState,
    pub cwd: PathBuf,
    pub branch: Option<String>,
    pub is_worktree: bool,
    pub started_at: Option<DateTime<Local>>,
    pub session_id: Option<String>,
    pub alias: Option<String>,
}

impl OverviewAgent {
    pub fn display_name(&self) -> &str {
        self.alias.as_deref().unwrap_or(self.id.tool().as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct OverviewStatusCounts {
    pub total: usize,
    pub running: usize,
    pub waiting: usize,
    pub error: usize,
    pub done: usize,
}

impl OverviewState {
    pub fn visible_agents(&self) -> Vec<&OverviewAgent> {
        self.visible_agent_indices()
            .into_iter()
            .map(|idx| &self.agents[idx])
            .collect()
    }

    pub fn visible_agent_count(&self) -> usize {
        self.visible_agent_indices().len()
    }

    pub fn selected_agent_id(&self) -> Option<AgentId> {
        let visible_idx = self.selected_index?;
        let real_idx = *self.visible_agent_indices().get(visible_idx)?;
        Some(self.agents[real_idx].id.clone())
    }

    pub fn clamp_selection(&mut self) {
        let count = self.visible_agent_count();
        if count == 0 {
            self.selected_index = None;
        } else if self.selected_index.is_none() {
            self.selected_index = Some(0);
        } else if let Some(idx) = self.selected_index {
            if idx >= count {
                self.selected_index = Some(count - 1);
            }
        }
    }

    pub fn normalized_status_counts(&self) -> OverviewStatusCounts {
        let mut counts = OverviewStatusCounts::default();
        for agent in self.visible_agents() {
            counts.total += 1;
            match agent.normalized.status {
                NormalizedStatus::Working => counts.running += 1,
                NormalizedStatus::NeedsInput => counts.waiting += 1,
                NormalizedStatus::Done => counts.done += 1,
                NormalizedStatus::Blocked | NormalizedStatus::Unknown => counts.error += 1,
            }
        }
        counts
    }

    fn visible_agent_indices(&self) -> Vec<usize> {
        let mut indices: Vec<usize> = self
            .agents
            .iter()
            .enumerate()
            .filter(|(_, agent)| {
                !(agent.id.tool() == AgentTool::Codex
                    && agent.id.is_session()
                    && agent.pid.is_none())
            })
            .map(|(idx, _)| idx)
            .collect();

        if let Some(filter) = self.filter.as_deref() {
            let filter = filter.to_lowercase();
            indices.retain(|&idx| Self::agent_matches_filter(&self.agents[idx], &filter));
        }

        match self.sort_mode {
            SortMode::Urgency => {
                indices.sort_by_key(|&idx| {
                    let agent = &self.agents[idx];
                    (std::cmp::Reverse(agent.normalized.status), agent.status)
                });
            }
            SortMode::Name => {
                indices.sort_by(|&left, &right| {
                    let a = &self.agents[left];
                    let b = &self.agents[right];
                    a.display_name()
                        .cmp(b.display_name())
                        .then_with(|| a.pid.cmp(&b.pid))
                        .then_with(|| a.id.session_id().cmp(&b.id.session_id()))
                });
            }
            SortMode::Elapsed => {
                indices.sort_by(|&left, &right| {
                    self.agents[left]
                        .started_at
                        .cmp(&self.agents[right].started_at)
                });
            }
        }

        indices
    }

    fn agent_matches_filter(agent: &OverviewAgent, filter: &str) -> bool {
        agent.display_name().to_lowercase().contains(filter)
            || agent.status.as_str().to_lowercase().contains(filter)
            || agent
                .normalized
                .status
                .as_str()
                .to_lowercase()
                .contains(filter)
            || agent
                .normalized
                .reason
                .as_ref()
                .map(|reason| reason.as_str().to_lowercase().contains(filter))
                .unwrap_or(false)
            || agent.cwd.to_string_lossy().to_lowercase().contains(filter)
            || agent
                .branch
                .as_deref()
                .map(|branch| branch.to_lowercase().contains(filter))
                .unwrap_or(false)
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
    pub mode: AccountsMode,
    pub selected_index: Option<usize>,
    pub entries: Vec<AccountEntry>,
    pub profiles_by_key: BTreeMap<AccountKey, AccountProfile>,
    pub quota_by_key: BTreeMap<AccountKey, QuotaSnapshot>,
    pub quota_loading: bool,
    pub provider_filter: ProviderFilter,
    pub status_message: Option<String>,
    pub active_codex_profile_id: Option<String>,
    pub active_claude_profile_id: Option<String>,
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
    SavingProfile {
        provider: Provider,
        profile_id: String,
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
    use chrono::Utc;
    use std::path::PathBuf;

    use super::*;
    use crate::app::effect::AccountProfile;
    use crate::domain::accounts::{
        ClaudeProfile, CodexProfile, Confidence, LimitState, Provider, QuotaSnapshot,
    };

    fn codex_profile(id: &str) -> CodexProfile {
        let now = Utc::now();
        CodexProfile {
            id: id.to_string(),
            label: format!("Codex {id}"),
            email: Some(format!("{id}@example.com")),
            plan: Some("pro".to_string()),
            notes: Some("notes".to_string()),
            enabled: true,
            last_used: None,
            created_at: now,
            updated_at: now,
            profile_dir: PathBuf::from(format!("/tmp/{id}")),
            auth_path: PathBuf::from(format!("/tmp/{id}/auth.json")),
        }
    }

    fn claude_profile(id: &str) -> ClaudeProfile {
        let now = Utc::now();
        ClaudeProfile {
            id: id.to_string(),
            label: format!("Claude {id}"),
            email: Some(format!("{id}@example.com")),
            plan: Some("max".to_string()),
            notes: Some("notes".to_string()),
            enabled: true,
            last_used: None,
            created_at: now,
            updated_at: now,
            profile_dir: PathBuf::from(format!("/tmp/{id}")),
            auth_path: PathBuf::from(format!("/tmp/{id}/.credentials.json")),
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
    fn accounts_state_can_store_profiles_and_quota_by_account_key() {
        let mut state = AccountsState::default();
        let codex_key = AccountKey::for_entry(&AccountEntry {
            provider: Provider::Codex,
            profile_id: "codex-main".to_string(),
        });
        let claude_key =
            AccountKey::for_profile(&AccountProfile::Claude(claude_profile("claude-main")));

        state.profiles_by_key.insert(
            codex_key.clone(),
            AccountProfile::Codex(codex_profile("codex-main")),
        );
        state.profiles_by_key.insert(
            claude_key.clone(),
            AccountProfile::Claude(claude_profile("claude-main")),
        );
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
            profile_id: "codex-main".to_string(),
        };
        let profile = AccountProfile::Codex(codex_profile("codex-main"));

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
