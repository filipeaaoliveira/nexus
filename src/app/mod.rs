use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::collections::VecDeque;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{anyhow, bail, Context, Result};
use chrono::Local;
use chrono::Utc;
use serde_json::Value;

use crate::codex_profiles::profile::{
    extract_auth_metadata, load_profile_metadata, save_profile_metadata, CodexProfile,
    ProfileMetadata,
};
use crate::codex_profiles::quota::{load_quota_cache, save_quota_cache, QuotaCache, QuotaSnapshot};
use crate::config::Config;
pub use crate::domain::accounts::{AccountEntry, Provider, ProviderFilter};
use crate::event::AgentEvent;
use crate::types::*;

pub mod action;
pub mod effect;
pub mod reducer;
pub mod state;

pub use action::{
    AccountsAction, Action, Action as SemanticAction, DetectorAction, OverviewAction, SystemAction,
    WorkspaceAction,
};
pub use effect::{AccountProfile, Effect, EffectResult, ProfileId};
pub use state::{
    AccountsConfirmAction, AccountsEditField, AccountsMode, AccountsState, DetectorHealth, Overlay,
    OverviewAgent, OverviewMode, OverviewState, OverviewStatusCounts, ProfileDraft, RootState,
    SystemState, Workflow, Workspace, WorkspaceState,
};

const MAX_RECENT_EVENTS: usize = 20;

/// Which destructive action is awaiting confirmation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmKind {
    DeleteProfile,
    WriteToHome,
}

/// Returns the length of the unified list without allocating a `Vec`.
fn unified_list_len(codex_ids: &[String], claude_ids: &[String], filter: ProviderFilter) -> usize {
    let n_codex = if filter != ProviderFilter::Claude {
        codex_ids.len()
    } else {
        0
    };
    let n_claude = if filter != ProviderFilter::Codex {
        claude_ids.len()
    } else {
        0
    };
    n_codex + n_claude
}

/// Build the unified list from codex + claude profile IDs, applying the filter.
/// Codex profiles come first; both groups preserve their order.
pub fn build_unified_list(
    codex_ids: &[String],
    claude_ids: &[String],
    filter: ProviderFilter,
) -> Vec<AccountEntry> {
    let mut entries = Vec::new();
    if filter != ProviderFilter::Claude {
        for id in codex_ids {
            entries.push(AccountEntry {
                provider: Provider::Codex,
                profile_id: id.clone(),
            });
        }
    }
    if filter != ProviderFilter::Codex {
        for id in claude_ids {
            entries.push(AccountEntry {
                provider: Provider::Claude,
                profile_id: id.clone(),
            });
        }
    }
    entries
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveTab {
    Overview,
    Accounts,
}

impl ActiveTab {
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

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum EditField {
    #[default]
    Label,
    Notes,
    Enabled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountsTabState {
    pub codex_profile_ids: Vec<String>,
    pub selected_index: Option<usize>,
    pub codex_active_profile_id: Option<String>,
    pub claude_profile_ids: Vec<String>,
    pub claude_active_profile_id: Option<String>,
    pub claude_quota_cache: Option<QuotaCache>,
    pub active_filter: ProviderFilter,
    pub filter_mode: bool,
    pub filter_input: String,
    pub status_message: Option<String>,
    pub edit_mode: bool,
    pub edit_label: String,
    pub edit_email: Option<String>,
    pub edit_plan: Option<String>,
    pub edit_notes: String,
    pub edit_enabled: bool,
    pub edit_field: EditField,
    pub quota: Option<QuotaSnapshot>,
    pub codex_quota_cache: Option<QuotaCache>,
    pub quota_loading: bool, // true when async fetch is in progress
    pub pending_confirm: Option<ConfirmKind>,
    pub status_message_expires_at: Option<Instant>,
}

impl Default for AccountsTabState {
    fn default() -> Self {
        Self {
            codex_profile_ids: Vec::new(),
            selected_index: None,
            codex_active_profile_id: None,
            claude_profile_ids: Vec::new(),
            claude_active_profile_id: None,
            claude_quota_cache: None,
            active_filter: ProviderFilter::All,
            filter_mode: false,
            filter_input: String::new(),
            status_message: None,
            edit_mode: false,
            edit_label: String::new(),
            edit_email: None,
            edit_plan: None,
            edit_notes: String::new(),
            edit_enabled: true,
            edit_field: EditField::Label,
            quota: None,
            codex_quota_cache: None,
            quota_loading: false,
            pending_confirm: None,
            status_message_expires_at: None,
        }
    }
}

impl AccountsTabState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn next_profile(&mut self) {
        let len = unified_list_len(
            &self.codex_profile_ids,
            &self.claude_profile_ids,
            self.active_filter,
        );
        if len == 0 {
            self.selected_index = None;
            self.quota = None;
            return;
        }

        let new_index = match self.selected_index {
            Some(i) => (i + 1) % len,
            None => 0,
        };
        self.selected_index = Some(new_index);
        self.load_quota_from_cache();
    }

    pub fn previous_profile(&mut self) {
        let len = unified_list_len(
            &self.codex_profile_ids,
            &self.claude_profile_ids,
            self.active_filter,
        );
        if len == 0 {
            self.selected_index = None;
            self.quota = None;
            return;
        }

        let new_index = match self.selected_index {
            Some(i) => (i + len - 1) % len,
            None => len - 1,
        };
        self.selected_index = Some(new_index);
        self.load_quota_from_cache();
    }

    pub fn set_active(&mut self, profile_id: Option<String>) {
        self.codex_active_profile_id = profile_id;
    }

    /// Load quota from cache for the currently selected profile.
    pub fn load_quota_from_cache(&mut self) {
        let list = build_unified_list(
            &self.codex_profile_ids,
            &self.claude_profile_ids,
            self.active_filter,
        );
        let entry = self.selected_index.and_then(|i| list.get(i));
        let Some(entry) = entry else {
            self.quota = None;
            return;
        };
        let quota = match entry.provider {
            Provider::Codex => self
                .codex_quota_cache
                .as_ref()
                .and_then(|c| c.by_profile.get(&entry.profile_id))
                .cloned(),
            Provider::Claude => self
                .claude_quota_cache
                .as_ref()
                .and_then(|c| c.by_profile.get(&entry.profile_id))
                .cloned(),
        };
        self.quota = quota;
    }

    pub fn start_edit(
        &mut self,
        label: &str,
        email: Option<&str>,
        plan: Option<&str>,
        notes: &str,
        enabled: bool,
    ) {
        self.edit_mode = true;
        self.edit_label = label.to_string();
        self.edit_email = email.map(String::from);
        self.edit_plan = plan.map(String::from);
        self.edit_notes = notes.to_string();
        self.edit_enabled = enabled;
        self.edit_field = EditField::Label;
    }

    pub fn set_status(&mut self, msg: impl Into<String>) {
        self.status_message = Some(msg.into());
        self.status_message_expires_at = Some(Instant::now() + std::time::Duration::from_secs(5));
    }

    pub fn clear_expired_status(&mut self) {
        if let Some(expires) = self.status_message_expires_at {
            if Instant::now() >= expires {
                self.status_message = None;
                self.status_message_expires_at = None;
            }
        }
    }

    pub fn cancel_edit(&mut self) {
        self.edit_mode = false;
        self.edit_label.clear();
        self.edit_email = None;
        self.edit_plan = None;
        self.edit_notes.clear();
        self.edit_enabled = true;
        self.edit_field = EditField::Label;
    }
}

#[derive(Debug, Default)]
struct VisibleAgentCache {
    version: u64,
    filter: Option<String>,
    sort_mode: Option<SortMode>,
    indices: Vec<usize>,
}

#[derive(Debug, Clone)]
struct SessionInfoUpdate {
    pid: Option<u32>,
    session_id: String,
    branch: Option<String>,
    is_worktree: bool,
    started_at: Option<chrono::DateTime<chrono::Local>>,
}

pub struct AppState {
    active_tab: ActiveTab,
    pub accounts_tab: AccountsTabState,
    pub agents: Vec<Agent>,
    pub selected: Option<usize>,
    pub sort_mode: SortMode,
    pub show_help: bool,
    pub help_scroll: u16,
    pub filter: Option<String>,
    pub filter_mode: bool,
    pub filter_input: String,
    pub rename_mode: bool,
    pub rename_input: String,
    pub config: Config,
    pub should_quit: bool,
    detector_issues: BTreeMap<String, String>,
    visible_agent_cache: RefCell<VisibleAgentCache>,
    agents_version: Cell<u64>,
    codex_profiles_last_reload: Cell<std::time::Instant>,
    claude_profiles_last_reload: Cell<std::time::Instant>,
    pub started_at: Instant,
}

/// Returns the next available "profile-NN" ID by scanning the profiles directory
/// on disk. Ensures no collision even after profiles are deleted.
fn next_profile_id(profiles_dir: &std::path::Path) -> String {
    let used: std::collections::BTreeSet<u32> = std::fs::read_dir(profiles_dir)
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .filter_map(|e| {
                    e.file_name()
                        .to_string_lossy()
                        .strip_prefix("profile-")
                        .and_then(|s| s.parse::<u32>().ok())
                })
                .collect()
        })
        .unwrap_or_default();

    let next = (1u32..).find(|n| !used.contains(n)).unwrap_or(1);
    format!("profile-{next:02}")
}

mod accounts;
mod events;
mod overview;

impl AppState {
    pub fn new(config: Config) -> Self {
        let mut accounts_tab = AccountsTabState::default();

        // Load cached quota from file
        if let Ok(cache) = load_quota_cache(&config.codex_quota_cache_file) {
            accounts_tab.codex_quota_cache = Some(cache);
        }
        if let Ok(cache) = load_quota_cache(&config.claude_quota_cache_file) {
            accounts_tab.claude_quota_cache = Some(cache);
        }

        let mut state = Self {
            active_tab: ActiveTab::Overview,
            accounts_tab,
            agents: Vec::new(),
            selected: None,
            sort_mode: SortMode::Name,
            show_help: false,
            help_scroll: 0,
            filter: None,
            filter_mode: false,
            filter_input: String::new(),
            rename_mode: false,
            rename_input: String::new(),
            config,
            should_quit: false,
            detector_issues: BTreeMap::new(),
            visible_agent_cache: RefCell::new(VisibleAgentCache::default()),
            agents_version: Cell::new(0),
            // Set to past so first reload always runs
            codex_profiles_last_reload: Cell::new(
                std::time::Instant::now() - std::time::Duration::from_secs(10),
            ),
            claude_profiles_last_reload: Cell::new(
                std::time::Instant::now() - std::time::Duration::from_secs(60),
            ),
            started_at: Instant::now(),
        };
        state.reload_codex_profiles();
        state.reload_claude_profiles();
        state
    }

    pub fn active_tab(&self) -> ActiveTab {
        self.active_tab
    }

    pub fn next_tab(&mut self) {
        self.active_tab = self.active_tab.next();
    }

    pub fn previous_tab(&mut self) {
        self.active_tab = self.active_tab.previous();
    }

    pub fn active_filter_mode(&self) -> bool {
        match self.active_tab {
            ActiveTab::Overview => self.filter_mode,
            ActiveTab::Accounts => self.accounts_tab.filter_mode,
        }
    }

    pub fn active_rename_mode(&self) -> bool {
        match self.active_tab {
            ActiveTab::Overview => self.rename_mode,
            ActiveTab::Accounts => false,
        }
    }

    pub fn handle_event(&mut self, event: AgentEvent) {
        let now = Local::now();
        match event {
            AgentEvent::DetectorHealthChanged {
                detector,
                healthy,
                message,
            } => self.handle_detector_health_changed(detector, healthy, message),
            AgentEvent::Discovered {
                id,
                pid,
                cwd,
                started_at,
            } => self.handle_discovered(id, pid, cwd, started_at, now),
            AgentEvent::StatusChanged { id, pid, old, new } => {
                self.handle_status_changed(id, pid, old, new, now)
            }
            AgentEvent::ActivityChanged { id, pid, activity } => {
                self.handle_activity_changed(id, pid, activity, now)
            }
            AgentEvent::NormalizedStateChanged { id, pid, state } => {
                self.handle_normalized_state_changed(id, pid, state, now)
            }
            AgentEvent::SubagentSpawned { id, pid, subagent } => {
                self.handle_subagent_spawned(id, pid, subagent, now)
            }
            AgentEvent::SubagentCompleted {
                id,
                pid,
                subagent_name,
            } => self.handle_subagent_completed(id, pid, subagent_name, now),
            AgentEvent::SubagentsUpdated { id, pid, subagents } => {
                self.handle_subagents_updated(id, pid, subagents, now)
            }
            AgentEvent::ToolCallStarted {
                id,
                pid,
                tool_name,
                args_summary,
            } => self.handle_tool_call_started(id, pid, tool_name, args_summary, now),
            AgentEvent::ToolCallCompleted { id, pid } => {
                self.handle_tool_call_completed(id, pid, now)
            }
            AgentEvent::ProcessExited { id, pid } => self.handle_process_exited(id, pid, now),
            AgentEvent::SessionInfo {
                id,
                pid,
                session_id,
                branch,
                is_worktree,
                started_at,
            } => self.handle_session_info(
                id,
                SessionInfoUpdate {
                    pid,
                    session_id,
                    branch,
                    is_worktree,
                    started_at,
                },
                now,
            ),
            AgentEvent::TodosUpdated { id, pid, todos } => {
                self.handle_todos_updated(id, pid, todos, now)
            }
        }
    }

    pub fn visible_agents(&self) -> Vec<&Agent> {
        self.with_visible_agent_indices(|indices| {
            indices.iter().map(|&idx| &self.agents[idx]).collect()
        })
    }

    pub fn count_by_status(&self, status: PrimaryStatus) -> usize {
        self.with_visible_agent_indices(|indices| {
            indices
                .iter()
                .filter(|&&idx| self.agents[idx].status == status)
                .count()
        })
    }

    pub fn primary_detector_issue(&self) -> Option<&str> {
        self.detector_issues
            .values()
            .next()
            .map(std::string::String::as_str)
    }
}

fn push_event(
    events: &mut VecDeque<ActivityEvent>,
    kind: ActivityEventKind,
    description: String,
    now: chrono::DateTime<Local>,
) {
    events.push_back(ActivityEvent {
        timestamp: now,
        kind,
        description,
    });
    while events.len() > MAX_RECENT_EVENTS {
        events.pop_front();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codex_profiles::profile::{
        load_profile_metadata, save_profile_metadata, CodexProfile, ProfileMetadata,
    };
    use crate::config::Config;
    use crate::event::AgentEvent;
    use crate::types::{AgentId, AgentTool, PrimaryStatus, SortMode};
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    #[test]
    fn test_next_profile_id_empty_dir() {
        let tmp = tempfile::TempDir::new().unwrap();
        assert_eq!(next_profile_id(tmp.path()), "profile-01");
    }

    #[test]
    fn test_next_profile_id_fills_gap() {
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::create_dir(tmp.path().join("profile-01")).unwrap();
        std::fs::create_dir(tmp.path().join("profile-03")).unwrap();
        // Gap at 02
        assert_eq!(next_profile_id(tmp.path()), "profile-02");
    }

    #[test]
    fn test_next_profile_id_sequential() {
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::create_dir(tmp.path().join("profile-01")).unwrap();
        std::fs::create_dir(tmp.path().join("profile-02")).unwrap();
        assert_eq!(next_profile_id(tmp.path()), "profile-03");
    }

    #[test]
    fn test_next_profile_id_nonexistent_dir() {
        let path = std::path::PathBuf::from("/tmp/nexus-test-nonexistent-dir-xyz");
        // read_dir fails → default to profile-01
        assert_eq!(next_profile_id(&path), "profile-01");
    }

    fn discovered(tool: AgentTool, pid: u32, cwd: &str) -> AgentEvent {
        AgentEvent::Discovered {
            id: AgentId::new(tool, pid),
            pid: Some(pid),
            cwd: PathBuf::from(cwd),
            started_at: None,
        }
    }

    #[test]
    fn visible_agent_indices_refresh_when_filter_changes() {
        let mut state = AppState::new(Config::default());
        state.handle_event(discovered(AgentTool::Claude, 1, "/tmp/claude"));
        state.handle_event(discovered(AgentTool::OpenCode, 2, "/tmp/opencode"));

        assert_eq!(state.visible_agent_count(), 2);

        state.filter = Some("claude".to_string());
        assert_eq!(state.visible_agent_count(), 1);
        assert_eq!(state.visible_agents()[0].pid, Some(1));

        state.filter = Some("opencode".to_string());
        assert_eq!(state.visible_agent_count(), 1);
        assert_eq!(state.visible_agents()[0].pid, Some(2));
    }

    #[test]
    fn visible_agent_indices_refresh_when_sort_changes() {
        let mut state = AppState::new(Config::default());
        state.handle_event(discovered(AgentTool::OpenCode, 2, "/tmp/opencode"));
        state.handle_event(discovered(AgentTool::Claude, 1, "/tmp/claude"));

        assert_eq!(state.visible_agents()[0].pid, Some(1));

        state.sort_mode = SortMode::Elapsed;
        assert_eq!(state.visible_agents()[0].pid, Some(2));

        state.handle_event(AgentEvent::StatusChanged {
            id: AgentId::new(AgentTool::OpenCode, 2),
            pid: Some(2),
            old: PrimaryStatus::Running,
            new: PrimaryStatus::Waiting,
        });
        state.sort_mode = SortMode::Urgency;
        assert_eq!(state.visible_agents()[0].pid, Some(2));
    }

    #[test]
    fn test_import_codex_auth_writes_profile_json() {
        // Set up a temp dir to act as the profiles dir and a fake auth.json
        let tmp = TempDir::new().unwrap();
        let profiles_dir = tmp.path().join("profiles");
        fs::create_dir_all(&profiles_dir).unwrap();

        let auth_source = tmp.path().join("auth.json");
        fs::write(&auth_source, b"{\"email\":\"test@example.com\"}").unwrap();

        // Exercise the same logic that import_codex_auth uses, calling helpers directly
        // (constructing a full AppState in a unit test is impractical)
        let profile_id = "profile-01";
        let profile_dir = profiles_dir.join(profile_id);
        let profile = CodexProfile::import_from_auth(
            profile_id,
            profile_id,
            profile_dir.clone(),
            &auth_source,
        )
        .unwrap();

        let metadata = ProfileMetadata {
            id: profile.id.clone(),
            label: Some(profile.label.clone()),
            email: profile.email.clone(),
            plan: profile.plan.clone(),
            notes: profile.notes.clone(),
            enabled: profile.enabled,
            created_at: Some(profile.created_at),
            updated_at: Some(profile.updated_at),
        };
        save_profile_metadata(&profile.profile_dir, &metadata).unwrap();

        // Verify profile.json was written with enabled=true
        let written = load_profile_metadata(&profile_dir).unwrap();
        assert!(
            written.enabled,
            "imported profile must have enabled=true in profile.json"
        );
        assert_eq!(written.label, Some(profile_id.to_string()));
    }

    #[test]
    fn test_build_unified_list_all_filter() {
        let codex = vec!["c1".to_string(), "c2".to_string()];
        let claude = vec!["cl1".to_string()];
        let list = build_unified_list(&codex, &claude, ProviderFilter::All);
        assert_eq!(list.len(), 3);
        assert_eq!(
            list[0],
            AccountEntry {
                provider: Provider::Codex,
                profile_id: "c1".to_string()
            }
        );
        assert_eq!(
            list[1],
            AccountEntry {
                provider: Provider::Codex,
                profile_id: "c2".to_string()
            }
        );
        assert_eq!(
            list[2],
            AccountEntry {
                provider: Provider::Claude,
                profile_id: "cl1".to_string()
            }
        );
    }

    #[test]
    fn test_build_unified_list_codex_filter() {
        let codex = vec!["c1".to_string()];
        let claude = vec!["cl1".to_string()];
        let list = build_unified_list(&codex, &claude, ProviderFilter::Codex);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].provider, Provider::Codex);
    }

    #[test]
    fn test_build_unified_list_claude_filter() {
        let codex = vec!["c1".to_string()];
        let claude = vec!["cl1".to_string()];
        let list = build_unified_list(&codex, &claude, ProviderFilter::Claude);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].provider, Provider::Claude);
    }

    #[test]
    fn test_provider_filter_cycle() {
        assert_eq!(ProviderFilter::All.next(), ProviderFilter::Codex);
        assert_eq!(ProviderFilter::Codex.next(), ProviderFilter::Claude);
        assert_eq!(ProviderFilter::Claude.next(), ProviderFilter::All);
    }

    #[test]
    fn test_build_unified_list_empty() {
        let list = build_unified_list(&[], &[], ProviderFilter::All);
        assert!(list.is_empty());
    }

    #[test]
    fn edit_field_cycle_includes_enabled() {
        let mut state = AppState::new(Config::default());
        state.accounts_tab.edit_field = EditField::Label;

        state.accounts_edit_next_field();
        assert_eq!(state.accounts_tab.edit_field, EditField::Notes);

        state.accounts_edit_next_field();
        assert_eq!(state.accounts_tab.edit_field, EditField::Enabled);

        state.accounts_edit_next_field();
        assert_eq!(state.accounts_tab.edit_field, EditField::Label);

        state.accounts_edit_prev_field();
        assert_eq!(state.accounts_tab.edit_field, EditField::Enabled);
    }
}
