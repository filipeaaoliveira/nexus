use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::Instant;

use chrono::Local;

use crate::config::Config;
pub use crate::domain::accounts::{
    AccountEntry, Provider, ProviderFilter, QuotaCache, QuotaSnapshot,
};
pub use crate::domain::agents::activity::{
    Activity, ActivityEvent, ActivityEventKind, Subagent, TodoItem, TodoPriority, TodoStatus,
};
pub use crate::domain::agents::ids::{AgentId, AgentIdentity, AgentTool};
pub use crate::domain::agents::model::{Agent, SortMode};
pub use crate::domain::agents::status::{
    NormalizedReason, NormalizedState, NormalizedStatus, PrimaryStatus,
};
pub use crate::domain::system::health::CodexBackendHealth;
use crate::event::AgentEvent;
use crate::services::accounts::quota::load_quota_cache;

pub mod action;
pub mod dispatch;
pub mod effect;
pub mod reducer;
pub mod state;

pub use action::{
    AccountsAction, Action, Action as SemanticAction, DetectorAction, OverviewAction, SystemAction,
    WorkspaceAction,
};
pub use effect::{AccountProfile, Effect, EffectResult, ProfileId, RefreshOrigin};
pub use state::{
    AccountsConfirmAction, AccountsEditField, AccountsMode, AccountsState, DetectorHealth, Overlay,
    OverviewAgent, OverviewMode, OverviewState, OverviewStatusCounts, ProfileDraft, RootState,
    SystemState, Workflow, Workspace, WorkspaceState,
};
pub use state::Workspace as ActiveTab;

const MAX_RECENT_EVENTS: usize = 20;

/// Which destructive action is awaiting confirmation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmKind {
    DeleteProfile,
    WriteToHome,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum EditField {
    #[default]
    Label,
    Notes,
    Enabled,
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
    pub root: RootState,
    pub agents: Vec<Agent>,
    pub config: Config,
    pub should_quit: bool,
    detector_issues: BTreeMap<String, String>,
    interactive_quota_refreshes: BTreeSet<state::AccountKey>,
    accounts_status_message_expires_at: Option<Instant>,
    visible_agent_cache: RefCell<VisibleAgentCache>,
    agents_version: Cell<u64>,
    pub started_at: Instant,
}

mod accounts;
mod events;
mod overview;

impl AppState {
    pub fn new(config: Config) -> Self {
        let mut root = RootState::default();

        // Load cached quota from file
        if let Ok(cache) = load_quota_cache(&config.codex_quota_cache_file) {
            for (profile_id, quota) in &cache.by_profile {
                root.accounts.quota_by_key.insert(
                    state::AccountKey::new(Provider::Codex, profile_id.clone()),
                    quota.clone(),
                );
            }
        }
        if let Ok(cache) = load_quota_cache(&config.claude_quota_cache_file) {
            for (profile_id, quota) in &cache.by_profile {
                root.accounts.quota_by_key.insert(
                    state::AccountKey::new(Provider::Claude, profile_id.clone()),
                    quota.clone(),
                );
            }
        }

        Self {
            root,
            agents: Vec::new(),
            config,
            should_quit: false,
            detector_issues: BTreeMap::new(),
            interactive_quota_refreshes: BTreeSet::new(),
            accounts_status_message_expires_at: None,
            visible_agent_cache: RefCell::new(VisibleAgentCache::default()),
            agents_version: Cell::new(0),
            started_at: Instant::now(),
        }
    }

    pub fn active_tab(&self) -> ActiveTab {
        self.root.workspace.active
    }

    pub fn next_tab(&mut self) {
        self.root.next_workspace();
    }

    pub fn previous_tab(&mut self) {
        self.root.previous_workspace();
    }

    pub fn show_help(&self) -> bool {
        matches!(self.root.system.overlay, Overlay::Help { .. })
    }

    pub fn help_scroll(&self) -> u16 {
        match self.root.system.overlay {
            Overlay::Help { scroll } => scroll,
            Overlay::None => 0,
        }
    }

    pub fn clear_expired_accounts_status(&mut self) {
        if let Some(expires) = self.accounts_status_message_expires_at {
            if Instant::now() >= expires {
                self.root.accounts.status_message = None;
                self.accounts_status_message_expires_at = None;
            }
        }
    }

    pub fn active_filter_mode(&self) -> bool {
        match self.active_tab() {
            ActiveTab::Overview => matches!(self.root.overview.mode, OverviewMode::Filtering { .. }),
            ActiveTab::Accounts => matches!(self.root.accounts.mode, AccountsMode::Filtering { .. }),
        }
    }

    pub fn active_rename_mode(&self) -> bool {
        match self.active_tab() {
            ActiveTab::Overview => matches!(self.root.overview.mode, OverviewMode::Renaming { .. }),
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
        self.sync_root_overview_agents();
        self.clamp_selection();
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
    use crate::domain::agents::ids::{AgentId, AgentTool};
    use crate::domain::agents::model::SortMode;
    use crate::domain::agents::status::PrimaryStatus;
    use crate::event::AgentEvent;
    use crate::services::accounts::storage::next_profile_id;
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

        assert_eq!(state.root.overview.visible_agent_count(), 2);

        state.root.overview.filter = Some("claude".to_string());
        assert_eq!(state.root.overview.visible_agent_count(), 1);
        assert_eq!(state.visible_agents()[0].pid, Some(1));

        state.root.overview.filter = Some("opencode".to_string());
        assert_eq!(state.root.overview.visible_agent_count(), 1);
        assert_eq!(state.visible_agents()[0].pid, Some(2));
    }

    #[test]
    fn visible_agent_indices_refresh_when_sort_changes() {
        let mut state = AppState::new(Config::default());
        state.handle_event(discovered(AgentTool::OpenCode, 2, "/tmp/opencode"));
        state.handle_event(discovered(AgentTool::Claude, 1, "/tmp/claude"));

        assert_eq!(state.visible_agents()[0].pid, Some(1));

        state.root.overview.sort_mode = SortMode::Elapsed;
        assert_eq!(state.visible_agents()[0].pid, Some(2));

        state.handle_event(AgentEvent::StatusChanged {
            id: AgentId::new(AgentTool::OpenCode, 2),
            pid: Some(2),
            old: PrimaryStatus::Running,
            new: PrimaryStatus::Waiting,
        });
        state.root.overview.sort_mode = SortMode::Urgency;
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
    fn test_provider_filter_cycle() {
        assert_eq!(ProviderFilter::All.next(), ProviderFilter::Codex);
        assert_eq!(ProviderFilter::Codex.next(), ProviderFilter::Claude);
        assert_eq!(ProviderFilter::Claude.next(), ProviderFilter::All);
    }

    #[test]
    fn accounts_state_visible_entries_respect_provider_filter() {
        let mut state = AccountsState {
            entries: vec![
                AccountEntry {
                    provider: Provider::Codex,
                    profile_id: "c1".to_string(),
                },
                AccountEntry {
                    provider: Provider::Codex,
                    profile_id: "c2".to_string(),
                },
                AccountEntry {
                    provider: Provider::Claude,
                    profile_id: "cl1".to_string(),
                },
            ],
            ..AccountsState::default()
        };

        assert_eq!(state.visible_entries().len(), 3);

        state.provider_filter = ProviderFilter::Codex;
        let codex_only = state.visible_entries();
        assert_eq!(codex_only.len(), 2);
        assert!(codex_only.iter().all(|entry| entry.provider == Provider::Codex));

        state.provider_filter = ProviderFilter::Claude;
        let claude_only = state.visible_entries();
        assert_eq!(claude_only.len(), 1);
        assert_eq!(claude_only[0].provider, Provider::Claude);
    }

    #[test]
    fn app_state_workspace_navigation_uses_root_workspace_state() {
        let mut state = AppState::new(Config::default());

        assert_eq!(state.root.workspace.active, Workspace::Overview);

        state.next_tab();
        assert_eq!(state.root.workspace.active, Workspace::Accounts);

        state.previous_tab();
        assert_eq!(state.root.workspace.active, Workspace::Overview);
    }

    #[test]
    fn app_state_help_overlay_uses_root_system_overlay() {
        let mut state = AppState::new(Config::default());

        assert_eq!(state.root.system.overlay, Overlay::None);

        state.toggle_help();
        assert_eq!(state.root.system.overlay, Overlay::Help { scroll: 0 });

        state.scroll_help_down();
        match state.root.system.overlay {
            Overlay::Help { scroll } => assert!(scroll > 0),
            Overlay::None => panic!("help overlay should stay visible while scrolling"),
        }

        state.toggle_help();
        assert_eq!(state.root.system.overlay, Overlay::None);
    }
}
