use std::cell::{Cell, RefCell};
use std::cmp::Reverse;
use std::collections::{BTreeSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use chrono::Local;
use chrono::Utc;
use serde_json::Value;

use crate::codex_profiles::profile::{
    extract_auth_metadata, load_profile_metadata, save_profile_metadata, CodexProfile,
    ProfileMetadata,
};
use crate::codex_profiles::quota::{load_quota_cache, save_quota_cache, QuotaCache, QuotaSnapshot};
use crate::codex_profiles::runtime::{
    canonical_repo_root, launch_codex, repo_runtime_key, require_codex_binary, RuntimePlan,
};
use crate::config::Config;
use crate::event::AgentEvent;
use crate::types::*;

const MAX_RECENT_EVENTS: usize = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    Codex,
    Claude,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ProviderFilter {
    #[default]
    All,
    Codex,
    Claude,
}

impl ProviderFilter {
    pub fn next(self) -> Self {
        match self {
            Self::All => Self::Codex,
            Self::Codex => Self::Claude,
            Self::Claude => Self::All,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Codex => "codex",
            Self::Claude => "claude",
        }
    }
}

/// A profile entry in the unified account list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountEntry {
    pub provider: Provider,
    pub profile_id: String,
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
            entries.push(AccountEntry { provider: Provider::Codex, profile_id: id.clone() });
        }
    }
    if filter != ProviderFilter::Codex {
        for id in claude_ids {
            entries.push(AccountEntry { provider: Provider::Claude, profile_id: id.clone() });
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
    pub session_picker_mode: bool,
    pub session_picker_selected: Option<usize>,
    pub session_picker_sessions: Option<Vec<String>>,
    pub edit_mode: bool,
    pub edit_label: String,
    pub edit_email: Option<String>,
    pub edit_plan: Option<String>,
    pub edit_notes: String,
    pub edit_enabled: bool,
    pub edit_field: EditField,
    pub view_mode: bool,
    pub quota: Option<QuotaSnapshot>,
    pub codex_quota_cache: Option<QuotaCache>,
    pub quota_edit_mode: bool,
    pub quota_five_hour: String,
    pub quota_weekly: String,
    pub quota_edit_field_weekly: bool, // false = 5h, true = weekly
    pub quota_loading: bool,           // true when async fetch is in progress
    pub pending_quota: Option<QuotaSnapshot>, // result from async fetch
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
            session_picker_mode: false,
            session_picker_selected: None,
            session_picker_sessions: None,
            edit_mode: false,
            edit_label: String::new(),
            edit_email: None,
            edit_plan: None,
            edit_notes: String::new(),
            edit_enabled: true,
            edit_field: EditField::Label,
            view_mode: false,
            quota: None,
            codex_quota_cache: None,
            quota_edit_mode: false,
            quota_five_hour: String::new(),
            quota_weekly: String::new(),
            quota_edit_field_weekly: false,
            quota_loading: false,
            pending_quota: None,
        }
    }
}

impl AccountsTabState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn next_profile(&mut self, codex_ids: &[String], claude_ids: &[String]) {
        let list = build_unified_list(codex_ids, claude_ids, self.active_filter);
        let len = list.len();
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
        self.load_quota_from_cache(codex_ids, claude_ids);
    }

    pub fn previous_profile(&mut self, codex_ids: &[String], claude_ids: &[String]) {
        let list = build_unified_list(codex_ids, claude_ids, self.active_filter);
        let len = list.len();
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
        self.load_quota_from_cache(codex_ids, claude_ids);
    }

    pub fn set_active(&mut self, profile_id: Option<String>) {
        self.codex_active_profile_id = profile_id;
    }

    /// Load quota from cache for currently selected profile
    pub fn load_quota_from_cache(&mut self, codex_ids: &[String], claude_ids: &[String]) {
        let list = build_unified_list(codex_ids, claude_ids, self.active_filter);
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

    /// Apply any pending quota result from async fetch
    pub fn apply_pending_quota(
        &mut self,
        profile_id: &str,
        quota: QuotaSnapshot,
        codex_ids: &[String],
        claude_ids: &[String],
    ) {
        let list = build_unified_list(codex_ids, claude_ids, self.active_filter);
        let is_selected = self
            .selected_index
            .and_then(|i| list.get(i))
            .map(|e| e.profile_id == profile_id)
            .unwrap_or(false);

        if is_selected {
            self.quota = Some(quota);
        }
        self.quota_loading = false;
        self.pending_quota = None;
    }

    /// Check if a profile is currently selected
    pub fn is_profile_selected(&self, profile_id: &str, codex_ids: &[String], claude_ids: &[String]) -> bool {
        let list = build_unified_list(codex_ids, claude_ids, self.active_filter);
        self.selected_index
            .and_then(|i| list.get(i))
            .map(|e| e.profile_id == profile_id)
            .unwrap_or(false)
    }

    /// Clear pending quota (fetch failed or was cancelled)
    pub fn clear_pending_quota(&mut self) {
        self.quota_loading = false;
        self.pending_quota = None;
    }

    /// Set pending quota from async fetch
    pub fn set_pending_quota(&mut self, quota: QuotaSnapshot) {
        self.pending_quota = Some(quota);
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
    pub filter: Option<String>,
    pub filter_mode: bool,
    pub filter_input: String,
    pub rename_mode: bool,
    pub rename_input: String,
    pub config: Config,
    pub should_quit: bool,
    visible_agent_cache: RefCell<VisibleAgentCache>,
    agents_version: Cell<u64>,
    codex_profiles_last_reload: Cell<std::time::Instant>,
    claude_profiles_last_reload: Cell<std::time::Instant>,
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

fn quota_state_to_input(state: &crate::codex_profiles::quota::LimitState) -> String {
    match state {
        crate::codex_profiles::quota::LimitState::Hit => "HIT".to_string(),
        crate::codex_profiles::quota::LimitState::Available => "ok".to_string(),
        crate::codex_profiles::quota::LimitState::Unknown => "?".to_string(),
    }
}

fn quota_input_to_window(
    label: &str,
    input: &str,
) -> crate::codex_profiles::quota::QuotaWindow {
    let state = match input.to_uppercase().as_str() {
        "HIT" => crate::codex_profiles::quota::LimitState::Hit,
        "?" => crate::codex_profiles::quota::LimitState::Unknown,
        _ => crate::codex_profiles::quota::LimitState::Available,
    };

    crate::codex_profiles::quota::QuotaWindow {
        label: label.to_string(),
        used_pct: None,
        reset_at: None,
        state,
    }
}

fn quota_summary_from_windows(windows: &[crate::codex_profiles::quota::QuotaWindow]) -> String {
    windows
        .iter()
        .map(|window| format!("{}={:?}", window.label, window.state))
        .collect::<Vec<_>>()
        .join("; ")
}

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
            filter: None,
            filter_mode: false,
            filter_input: String::new(),
            rename_mode: false,
            rename_input: String::new(),
            config,
            should_quit: false,
            visible_agent_cache: RefCell::new(VisibleAgentCache::default()),
            agents_version: Cell::new(0),
            // Set to past so first reload always runs
            codex_profiles_last_reload: Cell::new(
                std::time::Instant::now() - std::time::Duration::from_secs(10),
            ),
            claude_profiles_last_reload: Cell::new(
                std::time::Instant::now() - std::time::Duration::from_secs(60),
            ),
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
            AgentEvent::ToolCallStarted {
                id,
                pid,
                tool_name,
                args_summary,
            } => self.handle_tool_call_started(id, pid, tool_name, args_summary, now),
            AgentEvent::ToolCallCompleted { id, pid, tool_name } => {
                self.handle_tool_call_completed(id, pid, tool_name, now)
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

    pub fn move_selection_down(&mut self) {
        let count = self.visible_agent_count();
        if count == 0 {
            return;
        }
        let max = count - 1;
        self.selected = Some(match self.selected {
            Some(s) if s < max => s + 1,
            Some(_) => 0, // wrap to top
            None => 0,
        });
    }

    pub fn move_selection_up(&mut self) {
        let count = self.visible_agent_count();
        if count == 0 {
            return;
        }
        let max = count - 1;
        self.selected = Some(match self.selected {
            Some(s) if s > 0 => s - 1,
            Some(_) => max, // wrap to bottom
            None => 0,
        });
    }

    pub fn handle_codex_move_down(&mut self) {
        let codex_ids = self.accounts_tab.codex_profile_ids.clone();
        let claude_ids = self.accounts_tab.claude_profile_ids.clone();
        self.accounts_tab.next_profile(&codex_ids, &claude_ids);
    }

    pub fn handle_codex_move_up(&mut self) {
        let codex_ids = self.accounts_tab.codex_profile_ids.clone();
        let claude_ids = self.accounts_tab.claude_profile_ids.clone();
        self.accounts_tab.previous_profile(&codex_ids, &claude_ids);
    }

    pub fn cycle_sort_mode(&mut self) {
        self.sort_mode = self.sort_mode.next();
    }

    pub fn toggle_help(&mut self) {
        self.show_help = !self.show_help;
    }

    pub fn dismiss_overlay(&mut self) {
        self.show_help = false;
        self.filter_mode = false;
        self.filter = None;
        self.filter_input.clear();
        self.rename_mode = false;
        self.rename_input.clear();
        self.clamp_selection();
    }

    pub fn dismiss_codex_overlay(&mut self) {
        self.show_help = false;
        self.accounts_tab.filter_mode = false;
        self.accounts_tab.filter_input.clear();
    }

    pub fn start_filter(&mut self) {
        self.filter_mode = true;
        self.filter_input.clear();
        self.filter = None;
    }

    pub fn start_codex_filter(&mut self) {
        self.accounts_tab.filter_mode = true;
        self.accounts_tab.filter_input.clear();
    }

    pub fn start_session_picker(&mut self) {
        self.accounts_tab.session_picker_mode = true;
        self.accounts_tab.session_picker_selected = self.accounts_tab.selected_index;
    }

    pub fn dismiss_session_picker(&mut self) {
        self.accounts_tab.session_picker_mode = false;
        self.accounts_tab.session_picker_selected = None;
        self.accounts_tab.session_picker_sessions = None;
    }

    pub fn session_picker_move_up(&mut self) {
        let is_profiles = self.accounts_tab.session_picker_sessions.is_none();
        let items: &[String] = if is_profiles {
            &self.accounts_tab.codex_profile_ids
        } else {
            self.accounts_tab.session_picker_sessions.as_ref().unwrap()
        };

        let len = items.len();
        if len == 0 {
            self.accounts_tab.session_picker_selected = None;
            return;
        }
        let new_index = match self.accounts_tab.session_picker_selected {
            Some(index) => (index + len - 1) % len,
            None => len - 1,
        };
        self.accounts_tab.session_picker_selected = Some(new_index);
    }

    pub fn session_picker_move_down(&mut self) {
        let is_profiles = self.accounts_tab.session_picker_sessions.is_none();
        let items: &[String] = if is_profiles {
            &self.accounts_tab.codex_profile_ids
        } else {
            self.accounts_tab.session_picker_sessions.as_ref().unwrap()
        };

        let len = items.len();
        if len == 0 {
            self.accounts_tab.session_picker_selected = None;
            return;
        }
        let new_index = match self.accounts_tab.session_picker_selected {
            Some(index) => (index + 1) % len,
            None => 0,
        };
        self.accounts_tab.session_picker_selected = Some(new_index);
    }

    pub fn confirm_session_picker(&mut self) {
        if let Some(index) = self.accounts_tab.session_picker_selected {
            if self.accounts_tab.session_picker_sessions.is_some() {
                // Confirming session resume - extract session_id first
                let session_id = self
                    .accounts_tab
                    .session_picker_sessions
                    .as_ref()
                    .and_then(|sessions| sessions.get(index).map(|s| s.to_string()));

                self.dismiss_session_picker();

                if let Some(sid) = session_id {
                    self.resume_codex_session(&sid);
                }
            } else {
                // Confirming profile selection
                self.accounts_tab.selected_index = Some(index);
                self.accounts_tab.codex_active_profile_id = self.accounts_tab.codex_profile_ids.get(index).cloned();
                self.accounts_tab.view_mode = true;
                self.dismiss_session_picker();
                let codex_ids = self.accounts_tab.codex_profile_ids.clone();
                let claude_ids = self.accounts_tab.claude_profile_ids.clone();
                self.accounts_tab.load_quota_from_cache(&codex_ids, &claude_ids);
            }
        } else {
            self.dismiss_session_picker();
        }
    }

    pub fn get_repo_sessions(&self) -> Vec<String> {
        let cwd = match std::env::current_dir() {
            Ok(path) => path,
            Err(_) => return Vec::new(),
        };
        self.get_repo_sessions_for_cwd(&cwd)
    }

    pub fn get_repo_sessions_for_cwd(&self, cwd: &Path) -> Vec<String> {
        let current_repo_dir = self.current_repo_runtime_dir(cwd);
        let current_repo_root = canonical_repo_root(cwd).unwrap_or_else(|_| cwd.to_path_buf());
        let mut sessions = BTreeSet::new();

        self.collect_repo_sessions(&current_repo_dir, &mut sessions);
        self.collect_legacy_repo_sessions(&self.legacy_repo_runtime_dir(), &current_repo_root, &mut sessions);

        sessions.into_iter().collect()
    }

    pub fn start_session_picker_for_resume(&mut self) {
        let sessions = self.get_repo_sessions();
        if sessions.is_empty() {
            self.accounts_tab.status_message = Some("No sessions found in current repo".to_string());
            return;
        }

        self.accounts_tab.session_picker_mode = true;
        self.accounts_tab.session_picker_sessions = Some(sessions);
        self.accounts_tab.session_picker_selected = None;
    }

    pub fn start_codex_edit(&mut self) {
        use crate::claude_profiles::profile::load_claude_profile_metadata;

        let (provider, profile_id) = match self.selected_account() {
            Some((p, id)) => (p, id.to_string()),
            None => return,
        };

        let mut edit_email = None;
        let mut edit_plan = None;
        let mut label = profile_id.clone();
        let mut notes = String::new();
        let mut enabled = true;

        match provider {
            Provider::Codex => {
                let profile_dir = self.config.codex_profiles_dir.join(&profile_id);
                let auth_path = profile_dir.join("auth.json");
                if let Ok(auth_content) = fs::read_to_string(&auth_path) {
                    if let Ok(auth_json) = serde_json::from_str::<Value>(&auth_content) {
                        let auth_meta = extract_auth_metadata(&auth_json);
                        edit_email = auth_meta.email;
                        edit_plan = auth_meta.plan;
                    }
                }
                if let Ok(metadata) = load_profile_metadata(&profile_dir) {
                    label = metadata.label.unwrap_or(profile_id.clone());
                    notes = metadata.notes.unwrap_or_default();
                    enabled = metadata.enabled;
                }
            }
            Provider::Claude => {
                let profile_dir = self.config.claude_profiles_dir.join(&profile_id);
                if let Ok(metadata) = load_claude_profile_metadata(&profile_dir) {
                    label = metadata.label.unwrap_or(profile_id.clone());
                    edit_email = metadata.email;
                    edit_plan = metadata.plan;
                    notes = metadata.notes.unwrap_or_default();
                    enabled = metadata.enabled;
                }
            }
        }

        self.accounts_tab.start_edit(
            &label,
            edit_email.as_deref(),
            edit_plan.as_deref(),
            &notes,
            enabled,
        );
    }

    pub fn confirm_codex_edit(&mut self) -> Result<()> {
        use crate::claude_profiles::profile::{
            load_claude_profile_metadata, save_claude_profile_metadata, ClaudeProfileMetadata,
        };

        if !self.accounts_tab.edit_mode {
            return Ok(());
        }

        let (provider, profile_id) = match self.selected_account() {
            Some((p, id)) => (p, id.to_string()),
            None => return Err(anyhow!("no profile selected")),
        };

        match provider {
            Provider::Codex => {
                let profile_dir = self.config.codex_profiles_dir.join(&profile_id);
                let metadata = ProfileMetadata {
                    id: profile_id.clone(),
                    label: Some(self.accounts_tab.edit_label.clone()),
                    email: None,
                    plan: None,
                    notes: if self.accounts_tab.edit_notes.is_empty() {
                        None
                    } else {
                        Some(self.accounts_tab.edit_notes.clone())
                    },
                    enabled: self.accounts_tab.edit_enabled,
                    created_at: None,
                    updated_at: Some(Utc::now()),
                };
                save_profile_metadata(&profile_dir, &metadata)?;
            }
            Provider::Claude => {
                let profile_dir = self.config.claude_profiles_dir.join(&profile_id);
                let existing = load_claude_profile_metadata(&profile_dir).unwrap_or_default();
                let metadata = ClaudeProfileMetadata {
                    id: profile_id.clone(),
                    label: Some(self.accounts_tab.edit_label.clone()),
                    email: existing.email,
                    plan: existing.plan,
                    notes: if self.accounts_tab.edit_notes.is_empty() {
                        None
                    } else {
                        Some(self.accounts_tab.edit_notes.clone())
                    },
                    enabled: self.accounts_tab.edit_enabled,
                    created_at: existing.created_at,
                    updated_at: Some(Utc::now()),
                };
                save_claude_profile_metadata(&profile_dir, &metadata)?;
            }
        }

        self.accounts_tab.edit_mode = false;
        self.accounts_tab.status_message = Some(format!("Saved profile {}", profile_id));
        Ok(())
    }

    pub fn delete_codex_profile(&mut self) -> Result<()> {
        let profile_id = match self.selected_codex_profile_id() {
            Some(id) => id.to_string(),
            None => return Err(anyhow!("no profile selected")),
        };

        let profile_dir = self.config.codex_profiles_dir.join(&profile_id);
        if profile_dir.exists() {
            fs::remove_dir_all(&profile_dir)?;
        }

        if let Some(active) = &self.accounts_tab.codex_active_profile_id {
            if active == &profile_id {
                self.accounts_tab.codex_active_profile_id = None;
            }
        }

        self.accounts_tab.codex_profile_ids.retain(|id| id != &profile_id);
        self.clamp_accounts_selection();
        self.accounts_tab.view_mode = false;
        self.accounts_tab.status_message = Some(format!("Deleted profile {}", profile_id));
        Ok(())
    }

    pub fn delete_claude_profile(&mut self) -> anyhow::Result<()> {
        let profile_id = self
            .selected_claude_profile_id()
            .map(|s| s.to_string())
            .ok_or_else(|| anyhow::anyhow!("no Claude profile selected"))?;

        let profile_dir = self.config.claude_profiles_dir.join(&profile_id);
        if profile_dir.exists() {
            std::fs::remove_dir_all(&profile_dir)
                .with_context(|| format!("failed to delete {}", profile_dir.display()))?;
        }

        self.accounts_tab.claude_profile_ids.retain(|id| id != &profile_id);
        self.clamp_accounts_selection();
        self.accounts_tab.status_message = Some(format!("Deleted Claude {}", profile_id));
        Ok(())
    }

    pub fn cancel_codex_edit(&mut self) {
        self.accounts_tab.cancel_edit();
    }


    pub fn start_codex_quota_edit(&mut self) {
        let Some((_provider, _profile_id)) = self.selected_account() else {
            return;
        };

        // Initialize with the currently displayed windows when available, otherwise
        // fall back to the manual two-field editor.
        if let Some(ref quota) = self.accounts_tab.quota {
            let windows = quota.effective_windows();
            self.accounts_tab.quota_five_hour = windows
                .first()
                .map(|window| quota_state_to_input(&window.state))
                .unwrap_or_else(|| "ok".to_string());
            self.accounts_tab.quota_weekly = windows
                .get(1)
                .map(|window| quota_state_to_input(&window.state))
                .unwrap_or_else(|| "ok".to_string());
        } else {
            self.accounts_tab.quota_five_hour = "ok".to_string();
            self.accounts_tab.quota_weekly = "ok".to_string();
        }
        self.accounts_tab.quota_edit_mode = true;
    }

    pub fn dismiss_codex_quota_edit(&mut self) {
        self.accounts_tab.quota_edit_mode = false;
        self.accounts_tab.quota_edit_field_weekly = false;
    }

    pub fn codex_quota_edit_push_char(&mut self, _c: char) {
        // Space/enter cycles; this handles freeform input by cycling on any char
        self.cycle_quota_edit_field();
    }

    fn cycle_quota_edit_field(&mut self) {
        fn cycle(val: &str) -> &'static str {
            match val.to_uppercase().as_str() {
                "OK" => "HIT",
                "HIT" => "?",
                _ => "ok",
            }
        }
        if self.accounts_tab.quota_edit_field_weekly {
            let next = cycle(&self.accounts_tab.quota_weekly).to_string();
            self.accounts_tab.quota_weekly = next;
        } else {
            let next = cycle(&self.accounts_tab.quota_five_hour).to_string();
            self.accounts_tab.quota_five_hour = next;
        }
    }

    pub fn codex_quota_edit_pop_char(&mut self) {
        // Backspace also cycles (same as space)
        self.cycle_quota_edit_field();
    }

    pub fn codex_quota_edit_next_field(&mut self) {
        self.accounts_tab.quota_edit_field_weekly = !self.accounts_tab.quota_edit_field_weekly;
    }

    pub fn codex_quota_edit_prev_field(&mut self) {
        self.accounts_tab.quota_edit_field_weekly = !self.accounts_tab.quota_edit_field_weekly;
    }

    pub fn confirm_codex_quota_edit(&mut self) -> Result<()> {
        let profile_id = match self.selected_codex_profile_id() {
            Some(id) => id.to_string(),
            None => return Err(anyhow!("no profile selected")),
        };

        let windows = vec![
            quota_input_to_window("5h", &self.accounts_tab.quota_five_hour),
            quota_input_to_window("weekly", &self.accounts_tab.quota_weekly),
        ];
        let summary = quota_summary_from_windows(&windows);

        let quota = QuotaSnapshot::from_windows(
            Utc::now(),
            crate::codex_profiles::quota::Confidence::Cached,
            windows,
            None,
            summary,
        );

        // Save to cache
        let mut cache = self.accounts_tab.codex_quota_cache.take().unwrap_or_default();
        cache.by_profile.insert(profile_id.clone(), quota.clone());
        if let Err(e) = save_quota_cache(&cache, &self.config.codex_quota_cache_file) {
            tracing::warn!("failed to save codex quota cache: {}", e);
        }
        self.accounts_tab.codex_quota_cache = Some(cache);

        self.accounts_tab.quota = Some(quota);
        self.accounts_tab.quota_edit_mode = false;
        self.accounts_tab.status_message =
            Some(format!("Saved manual fallback quota for {}", profile_id));
        Ok(())
    }

    pub fn codex_edit_push_char(&mut self, c: char) {
        match self.accounts_tab.edit_field {
            EditField::Label => self.accounts_tab.edit_label.push(c),
            EditField::Notes => self.accounts_tab.edit_notes.push(c),
            EditField::Enabled => {}
        }
    }

    pub fn codex_edit_pop_char(&mut self) {
        match self.accounts_tab.edit_field {
            EditField::Label => {
                self.accounts_tab.edit_label.pop();
            }
            EditField::Notes => {
                self.accounts_tab.edit_notes.pop();
            }
            EditField::Enabled => {}
        }
    }

    pub fn codex_edit_toggle_enabled(&mut self) {
        self.accounts_tab.edit_enabled = !self.accounts_tab.edit_enabled;
    }

    pub fn codex_edit_next_field(&mut self) {
        self.accounts_tab.edit_field = EditField::Notes;
    }

    pub fn codex_edit_prev_field(&mut self) {
        self.accounts_tab.edit_field = EditField::Label;
    }

    pub fn dismiss_codex_edit(&mut self) {
        self.accounts_tab.cancel_edit();
    }

    pub fn filter_push_char(&mut self, c: char) {
        self.filter_input.push(c);
        self.sync_filter();
    }

    pub fn codex_filter_push_char(&mut self, c: char) {
        self.accounts_tab.filter_input.push(c);
    }

    pub fn filter_pop_char(&mut self) {
        self.filter_input.pop();
        self.sync_filter();
    }

    pub fn codex_filter_pop_char(&mut self) {
        self.accounts_tab.filter_input.pop();
    }

    fn sync_filter(&mut self) {
        self.filter = if self.filter_input.is_empty() {
            None
        } else {
            Some(self.filter_input.clone())
        };
        self.clamp_selection();
    }

    pub fn confirm_filter(&mut self) {
        self.filter_mode = false;
        // Keep filter active
        self.clamp_selection();
    }

    pub fn confirm_codex_filter(&mut self) {
        self.accounts_tab.filter_mode = false;
    }

    pub fn import_codex_auth(&mut self) -> Result<()> {
        let home = dirs::home_dir().ok_or_else(|| anyhow!("home directory not found"))?;
        let auth_source = home.join(".codex").join("auth.json");
        if !auth_source.is_file() {
            bail!("{} does not exist", auth_source.display());
        }

        fs::create_dir_all(&self.config.codex_profiles_dir).with_context(|| {
            format!(
                "failed to create {}",
                self.config.codex_profiles_dir.display()
            )
        })?;

        let profile_id = next_profile_id(&self.config.codex_profiles_dir);
        let profile_dir = self.config.codex_profiles_dir.join(&profile_id);
        let profile = CodexProfile::import_from_auth(
            &profile_id,
            &profile_id,
            profile_dir,
            auth_source.as_path(),
        )?;

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
        save_profile_metadata(&profile.profile_dir, &metadata)?;

        self.reload_codex_profiles();
        self.select_codex_profile_id(&profile.id);
        self.accounts_tab.status_message = Some(format!("Imported {}", profile.id));
        Ok(())
    }

    pub fn import_claude_auth(&mut self) -> anyhow::Result<()> {
        use crate::claude_profiles::profile::{
            ClaudeProfile, ClaudeProfileMetadata, save_claude_profile_metadata,
        };
        let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("home directory not found"))?;
        let creds_file = home.join(".claude").join(".credentials.json");

        let creds_source = if creds_file.is_file() {
            creds_file
        } else {
            let output = std::process::Command::new("security")
                .args(["find-generic-password", "-l", "Claude Code-credentials", "-w"])
                .output()
                .context("failed to run security CLI")?;
            if !output.status.success() {
                anyhow::bail!("No Claude credentials found. Run `claude` once to log in first.");
            }
            let json_str = String::from_utf8(output.stdout)
                .context("keychain output is not valid UTF-8")?
                .trim()
                .to_string();
            let tmp_path = std::env::temp_dir().join("nexus-claude-creds.json");
            std::fs::write(&tmp_path, json_str)?;
            tmp_path
        };

        std::fs::create_dir_all(&self.config.claude_profiles_dir).with_context(|| {
            format!("failed to create {}", self.config.claude_profiles_dir.display())
        })?;

        let profile_id = next_profile_id(&self.config.claude_profiles_dir);
        let profile_dir = self.config.claude_profiles_dir.join(&profile_id);
        let profile = ClaudeProfile::import_from_credentials(
            &profile_id,
            &profile_id,
            profile_dir.clone(),
            &creds_source,
        )?;

        let metadata = ClaudeProfileMetadata {
            id: profile.id.clone(),
            label: Some(profile.label.clone()),
            email: profile.email.clone(),
            plan: profile.plan.clone(),
            notes: profile.notes.clone(),
            enabled: profile.enabled,
            created_at: Some(profile.created_at),
            updated_at: Some(profile.updated_at),
        };
        save_claude_profile_metadata(&profile.profile_dir, &metadata)?;

        // Clean up temp credentials file if we created one
        let tmp_path = std::env::temp_dir().join("nexus-claude-creds.json");
        if tmp_path.exists() {
            let _ = std::fs::remove_file(&tmp_path);
        }

        self.reload_claude_profiles();
        self.accounts_tab.status_message = Some(format!("Imported Claude {}", profile.id));
        Ok(())
    }

    pub fn write_selected_claude_profile_to_home(&mut self) -> anyhow::Result<()> {
        let profile_id = self
            .selected_claude_profile_id()
            .map(|s| s.to_string())
            .ok_or_else(|| anyhow::anyhow!("no Claude profile selected"))?;
        let source = self
            .config
            .claude_profiles_dir
            .join(&profile_id)
            .join(".credentials.json");
        if !source.is_file() {
            anyhow::bail!("profile .credentials.json not found at {}", source.display());
        }
        let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("home directory not found"))?;
        let target = home.join(".claude").join(".credentials.json");
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        std::fs::copy(&source, &target)?;
        self.accounts_tab.status_message = Some(format!(
            "Wrote {} to ~/.claude/.credentials.json",
            profile_id
        ));
        Ok(())
    }

    pub fn cycle_accounts_filter(&mut self) {
        self.accounts_tab.active_filter = self.accounts_tab.active_filter.next();
        self.clamp_accounts_selection();
        let codex_ids = self.accounts_tab.codex_profile_ids.clone();
        let claude_ids = self.accounts_tab.claude_profile_ids.clone();
        self.accounts_tab.load_quota_from_cache(&codex_ids, &claude_ids);
    }

    pub fn launch_selected_codex_profile(&mut self) -> Result<()> {
        let profile_id = self
            .selected_codex_profile_id()
            .ok_or_else(|| anyhow!("no Codex profile selected"))?;
        let profile_dir = self.config.codex_profiles_dir.join(profile_id);
        let runtime_home = self.config.codex_runtime_dir.join(profile_id);
        let working_dir =
            std::env::current_dir().context("failed to resolve current working directory")?;
        let repo_dir = self.current_repo_runtime_dir(&working_dir);
        let runtime = RuntimePlan::new(profile_dir, repo_dir, runtime_home);
        let codex_bin = require_codex_binary(self.config.codex_binary.as_deref())?;

        let status = launch_codex(
            codex_bin.as_path(),
            &runtime,
            &self.config.codex_launch_args,
            working_dir.as_path(),
        )?;

        self.accounts_tab.status_message = Some(if status.success() {
            format!("Codex exited successfully for {profile_id}")
        } else {
            format!("Codex exited with status {status} for {profile_id}")
        });
        Ok(())
    }

    pub fn resume_codex_session(&mut self, session_id: &str) {
        let profile_id = match self.selected_codex_profile_id() {
            Some(id) => id.to_string(),
            None => {
                self.accounts_tab.status_message = Some("No profile selected".to_string());
                return;
            }
        };

        let profile_dir = self.config.codex_profiles_dir.join(&profile_id);
        let runtime_home = self.config.codex_runtime_dir.join(&profile_id);
        let working_dir = match std::env::current_dir() {
            Ok(d) => d,
            Err(e) => {
                self.accounts_tab.status_message = Some(format!("failed to resolve cwd: {}", e));
                return;
            }
        };
        let repo_dir = self.repo_runtime_dir_for_session(&working_dir, session_id);

        let runtime = RuntimePlan::new(profile_dir, repo_dir, runtime_home);

        let codex_bin = match require_codex_binary(self.config.codex_binary.as_deref()) {
            Ok(b) => b,
            Err(e) => {
                self.accounts_tab.status_message = Some(format!("failed to find codex: {}", e));
                return;
            }
        };

        let args = vec!["resume".to_string(), session_id.to_string()];

        ratatui::restore();
        let result = launch_codex(codex_bin.as_path(), &runtime, &args, working_dir.as_path());

        // Note: We can't re-init ratatui here since we don't have access to terminal
        // The caller should handle this
        match result {
            Ok(status) => {
                self.accounts_tab.status_message = Some(if status.success() {
                    format!("Resumed session {} with {}", session_id, profile_id)
                } else {
                    format!("Session exited with status {} for {}", status, profile_id)
                });
            }
            Err(e) => {
                self.accounts_tab.status_message = Some(format!("Failed to resume: {}", e));
            }
        }
    }

    pub fn reload_codex_profiles(&mut self) {
        // Skip if called within last 5 seconds to avoid repeated scans
        if self.codex_profiles_last_reload.get().elapsed() < std::time::Duration::from_secs(5) {
            return;
        }
        self.codex_profiles_last_reload
            .set(std::time::Instant::now());

        let selected_profile_id = self.selected_codex_profile_id().map(ToOwned::to_owned);
        let profiles_dir = self.config.codex_profiles_dir.clone();

        let profile_ids = tokio::task::block_in_place(|| {
            let mut ids = Vec::new();
            if let Ok(entries) = fs::read_dir(&profiles_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() && path.join("auth.json").is_file() {
                        if let Some(name) = path.file_name().and_then(|name| name.to_str()) {
                            ids.push(name.to_string());
                        }
                    }
                }
            }
            ids.sort();
            ids
        });

        self.accounts_tab.codex_profile_ids = profile_ids;

        // After setting profile IDs, select and load quota from cache
        if let Some(profile_id) = selected_profile_id.as_deref() {
            self.select_codex_profile_id(profile_id);
        } else {
            self.clamp_accounts_selection();
            // Try to load quota from cache for selected profile
            let codex_ids = self.accounts_tab.codex_profile_ids.clone();
            let claude_ids = self.accounts_tab.claude_profile_ids.clone();
            self.accounts_tab.load_quota_from_cache(&codex_ids, &claude_ids);
        }
    }

    pub fn reload_claude_profiles(&mut self) {
        if self.claude_profiles_last_reload.get().elapsed() < std::time::Duration::from_secs(5) {
            return;
        }
        self.claude_profiles_last_reload.set(std::time::Instant::now());

        let previously_selected = self.selected_claude_profile_id().map(|s| s.to_string());

        let profiles_dir = self.config.claude_profiles_dir.clone();
        let mut ids = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&profiles_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() && path.join(".credentials.json").is_file() {
                    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                        ids.push(name.to_string());
                    }
                }
            }
        }
        ids.sort();

        self.accounts_tab.claude_profile_ids = ids;

        // Re-select the previously selected Claude profile if it still exists
        if let Some(ref prev_id) = previously_selected {
            let list = build_unified_list(
                &self.accounts_tab.codex_profile_ids,
                &self.accounts_tab.claude_profile_ids,
                self.accounts_tab.active_filter,
            );
            if let Some(idx) = list.iter().position(|e| e.provider == Provider::Claude && &e.profile_id == prev_id) {
                self.accounts_tab.selected_index = Some(idx);
            } else {
                self.clamp_accounts_selection();
            }
        } else {
            self.clamp_accounts_selection();
        }

        let codex_ids = self.accounts_tab.codex_profile_ids.clone();
        let claude_ids = self.accounts_tab.claude_profile_ids.clone();
        self.accounts_tab.load_quota_from_cache(&codex_ids, &claude_ids);
    }

    /// Load quota from cache for currently selected profile
    pub fn load_quota_from_cache(&mut self) {
        let codex_ids = self.accounts_tab.codex_profile_ids.clone();
        let claude_ids = self.accounts_tab.claude_profile_ids.clone();
        self.accounts_tab.load_quota_from_cache(&codex_ids, &claude_ids);
    }

    pub fn selected_codex_profile_id(&self) -> Option<&str> {
        let list = build_unified_list(
            &self.accounts_tab.codex_profile_ids,
            &self.accounts_tab.claude_profile_ids,
            self.accounts_tab.active_filter,
        );
        let idx = self.accounts_tab.selected_index?;
        let entry = list.get(idx)?;
        if entry.provider == Provider::Codex {
            // SAFETY: profile_id comes from codex_profile_ids which is borrowed from self
            let pos = self.accounts_tab.codex_profile_ids.iter().position(|s| s == &entry.profile_id)?;
            Some(self.accounts_tab.codex_profile_ids[pos].as_str())
        } else {
            None
        }
    }

    pub fn selected_claude_profile_id(&self) -> Option<&str> {
        let list = build_unified_list(
            &self.accounts_tab.codex_profile_ids,
            &self.accounts_tab.claude_profile_ids,
            self.accounts_tab.active_filter,
        );
        let idx = self.accounts_tab.selected_index?;
        let entry = list.get(idx)?;
        if entry.provider == Provider::Claude {
            let pos = self.accounts_tab.claude_profile_ids.iter().position(|s| s == &entry.profile_id)?;
            Some(self.accounts_tab.claude_profile_ids[pos].as_str())
        } else {
            None
        }
    }

    pub fn selected_account(&self) -> Option<(Provider, &str)> {
        let list = build_unified_list(
            &self.accounts_tab.codex_profile_ids,
            &self.accounts_tab.claude_profile_ids,
            self.accounts_tab.active_filter,
        );
        let idx = self.accounts_tab.selected_index?;
        let entry = list.get(idx)?;
        let provider = entry.provider;
        let profile_id_ref = match provider {
            Provider::Codex => {
                let pos = self.accounts_tab.codex_profile_ids.iter().position(|s| s == &entry.profile_id)?;
                self.accounts_tab.codex_profile_ids[pos].as_str()
            }
            Provider::Claude => {
                let pos = self.accounts_tab.claude_profile_ids.iter().position(|s| s == &entry.profile_id)?;
                self.accounts_tab.claude_profile_ids[pos].as_str()
            }
        };
        Some((provider, profile_id_ref))
    }

    pub fn update_codex_quota_for_profile(&mut self, profile_id: &str, quota: QuotaSnapshot) {
        self.accounts_tab.quota_loading = false;
        let mut cache = self.accounts_tab.codex_quota_cache.take().unwrap_or_default();
        cache.by_profile.insert(profile_id.to_string(), quota.clone());
        if let Err(e) = save_quota_cache(&cache, &self.config.codex_quota_cache_file) {
            tracing::warn!("failed to save codex quota cache: {}", e);
        }
        self.accounts_tab.codex_quota_cache = Some(cache);
        if self.selected_codex_profile_id() == Some(profile_id) {
            self.accounts_tab.quota = Some(quota);
        }
    }

    pub fn update_claude_quota_for_profile(&mut self, profile_id: &str, quota: QuotaSnapshot) {
        self.accounts_tab.quota_loading = false;
        let mut cache = self.accounts_tab.claude_quota_cache.take().unwrap_or_default();
        cache.by_profile.insert(profile_id.to_string(), quota.clone());
        if let Err(e) = save_quota_cache(&cache, &self.config.claude_quota_cache_file) {
            tracing::warn!("failed to save claude quota cache: {}", e);
        }
        self.accounts_tab.claude_quota_cache = Some(cache);
        if self.selected_claude_profile_id() == Some(profile_id) {
            self.accounts_tab.quota = Some(quota);
        }
    }

    pub fn set_active_codex_profile(&mut self, profile_id: Option<String>) {
        self.accounts_tab.set_active(profile_id);
    }

    pub fn active_codex_profile_id(&self) -> Option<&str> {
        self.accounts_tab.codex_active_profile_id.as_deref()
    }

    pub fn write_selected_profile_to_codex_home(&mut self) -> Result<()> {
        let active_profile_id = self
            .selected_account()
            .filter(|(p, _)| *p == Provider::Codex)
            .map(|(_, id)| id.to_string())
            .ok_or_else(|| anyhow!("no codex profile selected"))?;

        let profile_dir = self.config.codex_profiles_dir.join(&active_profile_id);
        let source_auth = profile_dir.join("auth.json");

        if !source_auth.is_file() {
            bail!("profile auth.json not found at {}", source_auth.display());
        }

        let home = dirs::home_dir().ok_or_else(|| anyhow!("home directory not found"))?;
        let target_codex = home.join(".codex");
        let target_auth = target_codex.join("auth.json");

        fs::create_dir_all(&target_codex)?;
        fs::copy(&source_auth, &target_auth)?;

        self.accounts_tab.status_message =
            Some(format!("Wrote {} to ~/.codex/auth.json", active_profile_id));
        Ok(())
    }

    pub fn running_codex_sessions(&self) -> Vec<(String, Option<u32>)> {
        self.agents
            .iter()
            .filter(|a| {
                a.id.tool() == AgentTool::Codex
                    && a.id.is_session()
                    && a.status != PrimaryStatus::Exited
                    && a.session_id.as_ref().is_some_and(|s| !s.is_empty())
            })
            .map(|a| (a.session_id.clone().unwrap(), a.pid))
            .collect()
    }

    pub fn running_codex_sessions_for_cwd(&self, cwd: &Path) -> Vec<(String, Option<u32>)> {
        let current_repo_root = canonical_repo_root(cwd).unwrap_or_else(|_| cwd.to_path_buf());

        self.agents
            .iter()
            .filter(|a| {
                a.id.tool() == AgentTool::Codex
                    && a.id.is_session()
                    && a.status != PrimaryStatus::Exited
                    && a.session_id.as_ref().is_some_and(|s| !s.is_empty())
                    && canonical_repo_root(&a.cwd)
                        .map(|repo_root| repo_root == current_repo_root)
                        .unwrap_or(false)
            })
            .map(|a| (a.session_id.clone().unwrap(), a.pid))
            .collect()
    }

    pub fn restart_running_codex_with_profile(&mut self) -> Result<usize> {
        let active_profile_id = match self.active_codex_profile_id() {
            Some(id) => id,
            None => return Ok(0),
        };

        let working_dir =
            std::env::current_dir().context("failed to resolve current working directory")?;
        let running_sessions = self.running_codex_sessions_for_cwd(&working_dir);
        if running_sessions.is_empty() {
            return Ok(0);
        }

        let profile_dir = self.config.codex_profiles_dir.join(active_profile_id);
        let runtime_home = self.config.codex_runtime_dir.join(active_profile_id);
        let codex_bin = require_codex_binary(self.config.codex_binary.as_deref())?;

        let mut restarted = 0;
        for (session_id, _pid) in &running_sessions {
            let repo_dir = self.repo_runtime_dir_for_session(&working_dir, session_id);
            let runtime = RuntimePlan::new(
                profile_dir.clone(),
                repo_dir,
                runtime_home.clone(),
            );
            let args = vec!["resume".to_string(), session_id.clone()];
            let status = launch_codex(codex_bin.as_path(), &runtime, &args, working_dir.as_path())?;

            if status.success() {
                restarted += 1;
            }
        }

        self.accounts_tab.status_message = Some(format!(
            "Restarted {} session(s) with {}",
            restarted, active_profile_id
        ));
        Ok(restarted)
    }

    pub fn current_repo_runtime_dir(&self, cwd: &Path) -> PathBuf {
        let repo_root = canonical_repo_root(cwd).unwrap_or_else(|_| cwd.to_path_buf());
        self.config.codex_repos_dir.join(repo_runtime_key(&repo_root))
    }

    pub fn repo_runtime_dir_for_session(&self, cwd: &Path, session_id: &str) -> PathBuf {
        let current_repo_dir = self.current_repo_runtime_dir(cwd);
        let legacy_repo_dir = self.legacy_repo_runtime_dir();
        let current_repo_root = canonical_repo_root(cwd).unwrap_or_else(|_| cwd.to_path_buf());

        if self.session_exists_in_repo_dir(&current_repo_dir, session_id) {
            current_repo_dir
        } else if self.legacy_session_matches_repo(&legacy_repo_dir, session_id, &current_repo_root) {
            legacy_repo_dir
        } else {
            current_repo_dir
        }
    }

    fn legacy_repo_runtime_dir(&self) -> PathBuf {
        self.config.codex_repos_dir.join("current-repo")
    }

    fn session_exists_in_repo_dir(&self, repo_dir: &Path, session_id: &str) -> bool {
        repo_dir.join("sessions").join(session_id).is_dir()
    }

    fn legacy_session_matches_repo(
        &self,
        repo_dir: &Path,
        session_id: &str,
        current_repo_root: &Path,
    ) -> bool {
        let session_dir = repo_dir.join("sessions").join(session_id);
        session_dir.is_dir() && self.legacy_session_belongs_to_repo(&session_dir, current_repo_root)
    }

    fn collect_repo_sessions(&self, repo_dir: &Path, sessions: &mut BTreeSet<String>) {
        let sessions_dir = repo_dir.join("sessions");
        if !sessions_dir.is_dir() {
            return;
        }

        if let Ok(year_entries) = fs::read_dir(&sessions_dir) {
            for year_entry in year_entries.flatten() {
                let year_path = year_entry.path();
                if !year_path.is_dir() {
                    continue;
                }
                let year_name = match year_path.file_name().and_then(|n| n.to_str()) {
                    Some(n) => n.to_string(),
                    None => continue,
                };

                if let Ok(month_entries) = fs::read_dir(&year_path) {
                    for month_entry in month_entries.flatten() {
                        let month_path = month_entry.path();
                        if !month_path.is_dir() {
                            continue;
                        }
                        let month_name = match month_path.file_name().and_then(|n| n.to_str()) {
                            Some(n) => n.to_string(),
                            None => continue,
                        };

                        if let Ok(session_entries) = fs::read_dir(&month_path) {
                            for session_entry in session_entries.flatten() {
                                let session_path = session_entry.path();
                                if !session_path.is_dir() {
                                    continue;
                                }
                                let session_name =
                                    match session_path.file_name().and_then(|n| n.to_str()) {
                                        Some(n) if !n.starts_with('.') => n.to_string(),
                                        _ => continue,
                                    };
                                sessions.insert(format!(
                                    "{}/{}/{}",
                                    year_name, month_name, session_name
                                ));
                            }
                        }
                    }
                }
            }
        }
    }

    fn collect_legacy_repo_sessions(
        &self,
        repo_dir: &Path,
        current_repo_root: &Path,
        sessions: &mut BTreeSet<String>,
    ) {
        let sessions_dir = repo_dir.join("sessions");
        if !sessions_dir.is_dir() {
            return;
        }

        if let Ok(year_entries) = fs::read_dir(&sessions_dir) {
            for year_entry in year_entries.flatten() {
                let year_path = year_entry.path();
                if !year_path.is_dir() {
                    continue;
                }
                let year_name = match year_path.file_name().and_then(|n| n.to_str()) {
                    Some(n) => n.to_string(),
                    None => continue,
                };

                if let Ok(month_entries) = fs::read_dir(&year_path) {
                    for month_entry in month_entries.flatten() {
                        let month_path = month_entry.path();
                        if !month_path.is_dir() {
                            continue;
                        }
                        let month_name = match month_path.file_name().and_then(|n| n.to_str()) {
                            Some(n) => n.to_string(),
                            None => continue,
                        };

                        if let Ok(session_entries) = fs::read_dir(&month_path) {
                            for session_entry in session_entries.flatten() {
                                let session_path = session_entry.path();
                                if !session_path.is_dir() {
                                    continue;
                                }
                                let session_name =
                                    match session_path.file_name().and_then(|n| n.to_str()) {
                                        Some(n) if !n.starts_with('.') => n.to_string(),
                                        _ => continue,
                                    };
                                let session_id =
                                    format!("{}/{}/{}", year_name, month_name, session_name);
                                if sessions.contains(&session_id) {
                                    continue;
                                }
                                if self.legacy_session_belongs_to_repo(
                                    &session_path,
                                    current_repo_root,
                                ) {
                                    sessions.insert(session_id);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    fn legacy_session_belongs_to_repo(&self, session_dir: &Path, current_repo_root: &Path) -> bool {
        self.session_recorded_cwd(session_dir)
            .and_then(|cwd| canonical_repo_root(&cwd).ok())
            .is_some_and(|repo_root| repo_root == current_repo_root)
    }

    fn session_recorded_cwd(&self, session_dir: &Path) -> Option<PathBuf> {
        let mut stack = vec![session_dir.to_path_buf()];

        while let Some(path) = stack.pop() {
            let entries = fs::read_dir(&path).ok()?;
            for entry in entries.flatten() {
                let entry_path = entry.path();
                if entry_path.is_dir() {
                    stack.push(entry_path);
                    continue;
                }
                let content = fs::read_to_string(&entry_path).ok()?;
                if let Some(cwd) = self.extract_cwd_from_content(&content) {
                    return Some(PathBuf::from(cwd));
                }
            }
        }

        None
    }

    fn extract_cwd_from_content(&self, content: &str) -> Option<String> {
        if let Ok(value) = serde_json::from_str::<Value>(content) {
            if let Some(cwd) = Self::extract_cwd_from_value(&value) {
                return Some(cwd);
            }
        }

        for line in content.lines() {
            if let Ok(value) = serde_json::from_str::<Value>(line) {
                if let Some(cwd) = Self::extract_cwd_from_value(&value) {
                    return Some(cwd);
                }
            }
        }

        None
    }

    fn extract_cwd_from_value(value: &Value) -> Option<String> {
        match value {
            Value::Object(map) => {
                if let Some(cwd) = map.get("cwd").and_then(Value::as_str) {
                    return Some(cwd.to_string());
                }
                map.values().find_map(Self::extract_cwd_from_value)
            }
            Value::Array(items) => items.iter().find_map(Self::extract_cwd_from_value),
            _ => None,
        }
    }

    fn select_codex_profile_id(&mut self, profile_id: &str) {
        let list = build_unified_list(
            &self.accounts_tab.codex_profile_ids,
            &self.accounts_tab.claude_profile_ids,
            self.accounts_tab.active_filter,
        );
        self.accounts_tab.selected_index = list
            .iter()
            .position(|e| e.provider == Provider::Codex && e.profile_id == profile_id);
        self.clamp_accounts_selection();
    }

    fn clamp_accounts_selection(&mut self) {
        let list = build_unified_list(
            &self.accounts_tab.codex_profile_ids,
            &self.accounts_tab.claude_profile_ids,
            self.accounts_tab.active_filter,
        );
        let len = list.len();
        self.accounts_tab.selected_index = match (self.accounts_tab.selected_index, len) {
            (_, 0) => None,
            (Some(i), len) if i < len => Some(i),
            _ => Some(0),
        };
    }

    /// Resolve the currently selected visible index to the corresponding
    /// `AgentId` and its position in `self.agents`.
    fn resolve_selected(&self) -> Option<(usize, AgentId)> {
        let vis_idx = self.selected?;
        self.with_visible_agent_indices(|indices| {
            let real_idx = *indices.get(vis_idx)?;
            let id = self.agents[real_idx].id.clone();
            Some((real_idx, id))
        })
    }

    pub fn start_rename(&mut self) {
        let (real_idx, _id) = match self.resolve_selected() {
            Some(v) => v,
            None => return,
        };
        self.rename_mode = true;
        self.rename_input = self.agents[real_idx].display_name().to_string();
    }

    pub fn rename_push_char(&mut self, c: char) {
        self.rename_input.push(c);
    }

    pub fn rename_pop_char(&mut self) {
        self.rename_input.pop();
    }

    pub fn confirm_rename(&mut self) {
        if let Some((real_idx, _id)) = self.resolve_selected() {
            if real_idx < self.agents.len() {
                if self.rename_input.is_empty() {
                    self.agents[real_idx].alias = None;
                } else {
                    self.agents[real_idx].alias = Some(self.rename_input.clone());
                }
                self.mark_agents_dirty();
            }
        }
        self.rename_mode = false;
        self.rename_input.clear();
    }

    pub fn cancel_rename(&mut self) {
        self.rename_mode = false;
        self.rename_input.clear();
    }

    pub fn remove_exited(&mut self) {
        self.agents.retain(|a| a.status != PrimaryStatus::Exited);
        self.mark_agents_dirty();
        self.clamp_selection();
    }

    pub fn remove_selected(&mut self) {
        let (real_idx, _id) = match self.resolve_selected() {
            Some(v) => v,
            None => return,
        };
        self.agents.remove(real_idx);
        self.mark_agents_dirty();
        self.clamp_selection();
    }

    fn clamp_selection(&mut self) {
        let count = self.visible_agent_count();
        if count == 0 {
            self.selected = None;
        } else if self.selected.is_none() {
            self.selected = Some(0);
        } else if let Some(idx) = self.selected {
            if idx >= count {
                self.selected = Some(count - 1);
            }
        }
    }

    fn find_agent_mut_by_id(&mut self, id: &AgentId) -> Option<&mut Agent> {
        self.agents.iter_mut().find(|a| a.id == *id)
    }

    fn insert_placeholder_agent(
        &mut self,
        id: AgentId,
        pid: Option<u32>,
        now: chrono::DateTime<chrono::Local>,
    ) -> &mut Agent {
        if let Some(agent_idx) = self.agents.iter().position(|agent| agent.id == id) {
            return &mut self.agents[agent_idx];
        }

        let session_id = id.session_id().map(str::to_string);
        self.agents.push(Agent {
            id,
            pid,
            status: PrimaryStatus::Running,
            normalized: NormalizedState::new(NormalizedStatus::Working, None),
            activity: None,
            cwd: PathBuf::new(),
            branch: None,
            is_worktree: false,
            started_at: now,
            last_activity: now,
            subagents: Vec::new(),
            recent_events: VecDeque::new(),
            session_id,
            alias: None,
            todos: Vec::new(),
        });
        self.agents
            .last_mut()
            .expect("placeholder agent was just inserted")
    }

    fn find_process_agent_mut(&mut self, tool: AgentTool, pid: u32) -> Option<&mut Agent> {
        self.agents
            .iter_mut()
            .find(|a| a.id.tool() == tool && a.pid == Some(pid) && a.id.is_process())
    }

    fn find_session_agent_indices_by_pid(
        &self,
        tool: AgentTool,
        pid: u32,
        cwd: &Path,
    ) -> Vec<usize> {
        self.agents
            .iter()
            .enumerate()
            .filter(|(_, a)| {
                a.id.tool() == tool
                    && a.id.is_session()
                    && a.pid == Some(pid)
                    && a.status != PrimaryStatus::Exited
                    && (a.cwd.as_os_str().is_empty() || a.cwd == cwd)
            })
            .map(|(idx, _)| idx)
            .collect()
    }

    fn find_agent_mut_by_pid(&mut self, tool: AgentTool, pid: u32) -> Option<&mut Agent> {
        self.agents
            .iter_mut()
            .find(|a| a.id.tool() == tool && a.pid == Some(pid))
    }

    fn remove_redundant_codex_process_agents(&mut self, pid: Option<u32>) {
        let Some(pid) = pid else {
            return;
        };

        let len_before = self.agents.len();
        self.agents.retain(|agent| {
            !(agent.id.tool() == AgentTool::Codex
                && agent.id.is_process()
                && agent.pid == Some(pid))
        });

        if self.agents.len() != len_before {
            self.mark_agents_dirty();
            self.clamp_selection();
        }
    }

    fn handle_discovered(
        &mut self,
        id: AgentId,
        pid: Option<u32>,
        cwd: PathBuf,
        real_start: Option<chrono::DateTime<chrono::Local>>,
        now: chrono::DateTime<chrono::Local>,
    ) {
        let pid = pid.or_else(|| id.pid());
        let preserve_codex_session_status = id.tool() == AgentTool::Codex && id.is_process();
        if let Some(agent_idx) = self.agents.iter().position(|a| a.id == id) {
            let absorbed_pid = {
                let agent = &mut self.agents[agent_idx];
                agent.cwd = cwd;
                agent.pid = pid.or(agent.pid);
                if !(preserve_codex_session_status && agent.id.is_session()) {
                    agent.status = PrimaryStatus::Running;
                }
                if let Some(real_start) = real_start {
                    if real_start < agent.started_at {
                        agent.started_at = real_start;
                    }
                }
                agent.last_activity = now;
                agent.pid
            };
            self.mark_agents_dirty();
            if id.tool() == AgentTool::Codex && id.is_session() {
                self.remove_redundant_codex_process_agents(absorbed_pid);
            }
            self.clamp_selection();
            return;
        }

        let session_id = id.session_id().map(str::to_string);
        if id.tool() == AgentTool::Codex {
            if let Some(pid) = pid {
                let matching_agent_indices =
                    self.find_session_agent_indices_by_pid(id.tool(), pid, &cwd);
                if matching_agent_indices.len() > 1 {
                    for agent_idx in matching_agent_indices {
                        let agent = &mut self.agents[agent_idx];
                        agent.cwd = cwd.clone();
                        agent.pid = Some(pid);
                        if !preserve_codex_session_status {
                            agent.status = PrimaryStatus::Running;
                        }
                        if let Some(real_start) = real_start {
                            if real_start < agent.started_at {
                                agent.started_at = real_start;
                            }
                        }
                        agent.last_activity = now;
                    }
                    self.mark_agents_dirty();
                    self.clamp_selection();
                    return;
                }
                if let Some(agent_idx) = matching_agent_indices.into_iter().next() {
                    let agent = &mut self.agents[agent_idx];
                    agent.cwd = cwd;
                    agent.pid = Some(pid);
                    if !preserve_codex_session_status {
                        agent.status = PrimaryStatus::Running;
                    }
                    if let Some(real_start) = real_start {
                        if real_start < agent.started_at {
                            agent.started_at = real_start;
                        }
                    }
                    agent.last_activity = now;
                    self.mark_agents_dirty();
                    self.clamp_selection();
                    return;
                }
            }
        }

        self.agents.push(Agent {
            id,
            pid,
            status: PrimaryStatus::Running,
            normalized: NormalizedState::new(NormalizedStatus::Working, None),
            activity: None,
            cwd,
            branch: None,
            is_worktree: false,
            started_at: real_start.unwrap_or(now),
            last_activity: now,
            subagents: Vec::new(),
            recent_events: VecDeque::new(),
            session_id,
            alias: None,
            todos: Vec::new(),
        });
        self.mark_agents_dirty();
        if self.selected.is_none() {
            self.selected = Some(0);
        }
        self.clamp_selection();
    }

    fn handle_status_changed(
        &mut self,
        id: AgentId,
        pid: Option<u32>,
        old: PrimaryStatus,
        new: PrimaryStatus,
        now: chrono::DateTime<chrono::Local>,
    ) {
        if let Some(agent) = self.find_agent_mut_by_id(&id) {
            agent.pid = pid.or(agent.pid);
            agent.status = new;
            agent.last_activity = now;
            push_event(
                &mut agent.recent_events,
                ActivityEventKind::StatusChanged,
                format!("{} → {}", old.as_str(), new.as_str()),
                now,
            );
            self.mark_agents_dirty();
        }
    }

    fn handle_activity_changed(
        &mut self,
        id: AgentId,
        pid: Option<u32>,
        activity: Option<Activity>,
        now: chrono::DateTime<chrono::Local>,
    ) {
        if let Some(agent) = self.find_agent_mut_by_id(&id) {
            agent.pid = pid.or(agent.pid);
            agent.activity = activity;
            agent.last_activity = now;
            self.mark_agents_dirty();
        }
    }

    fn handle_normalized_state_changed(
        &mut self,
        id: AgentId,
        pid: Option<u32>,
        state: NormalizedState,
        now: chrono::DateTime<chrono::Local>,
    ) {
        if let Some(agent) = self.find_agent_mut_by_id(&id) {
            agent.pid = pid.or(agent.pid);
            agent.normalized = state;
            agent.last_activity = now;
        } else {
            let agent = self.insert_placeholder_agent(id, pid, now);
            agent.normalized = state;
        }
        self.mark_agents_dirty();
        self.clamp_selection();
    }

    fn handle_subagent_spawned(
        &mut self,
        id: AgentId,
        pid: Option<u32>,
        subagent: Subagent,
        now: chrono::DateTime<chrono::Local>,
    ) {
        if let Some(agent) = self.find_agent_mut_by_id(&id) {
            agent.pid = pid.or(agent.pid);
            let desc = format!("spawned: {}", subagent.name);
            agent.subagents.push(subagent);
            agent.last_activity = now;
            push_event(
                &mut agent.recent_events,
                ActivityEventKind::SubagentSpawned,
                desc,
                now,
            );
            self.mark_agents_dirty();
        }
    }

    fn handle_subagent_completed(
        &mut self,
        id: AgentId,
        pid: Option<u32>,
        subagent_name: String,
        now: chrono::DateTime<chrono::Local>,
    ) {
        if let Some(agent) = self.find_agent_mut_by_id(&id) {
            agent.pid = pid.or(agent.pid);
            let matching_subagent = if id.tool() == AgentTool::Codex {
                agent.subagents.iter_mut().find(|s| {
                    s.name == subagent_name
                        || s.description.as_deref() == Some(subagent_name.as_str())
                })
            } else {
                agent.subagents.iter_mut().find(|s| s.name == subagent_name)
            };
            if let Some(sub) = matching_subagent {
                sub.status = PrimaryStatus::Done;
                sub.ended_at = Some(now);
            }
            agent.last_activity = now;
            push_event(
                &mut agent.recent_events,
                ActivityEventKind::SubagentCompleted,
                format!("completed: {}", subagent_name),
                now,
            );
            self.mark_agents_dirty();
        }
    }

    fn handle_tool_call_started(
        &mut self,
        id: AgentId,
        pid: Option<u32>,
        tool_name: String,
        args_summary: Option<String>,
        now: chrono::DateTime<chrono::Local>,
    ) {
        if let Some(agent) = self.find_agent_mut_by_id(&id) {
            agent.pid = pid.or(agent.pid);
            agent.activity = Some(Activity::ToolCall);
            agent.last_activity = now;
            let desc = match args_summary {
                Some(args) => format!("{} ({})", tool_name, args),
                None => tool_name,
            };
            push_event(
                &mut agent.recent_events,
                ActivityEventKind::ToolCall,
                desc,
                now,
            );
            self.mark_agents_dirty();
        }
    }

    fn handle_tool_call_completed(
        &mut self,
        id: AgentId,
        pid: Option<u32>,
        _tool_name: String,
        now: chrono::DateTime<chrono::Local>,
    ) {
        if let Some(agent) = self.find_agent_mut_by_id(&id) {
            agent.pid = pid.or(agent.pid);
            if agent.activity == Some(Activity::ToolCall) {
                agent.activity = Some(Activity::Idle);
            }
            agent.last_activity = now;
            self.mark_agents_dirty();
        }
    }

    fn handle_process_exited(
        &mut self,
        id: AgentId,
        pid: Option<u32>,
        now: chrono::DateTime<chrono::Local>,
    ) {
        // Claude and OpenCode agents are always process-type; remove them immediately
        // so they leave the dashboard when the session closes.
        if (id.tool() == AgentTool::Claude || id.tool() == AgentTool::OpenCode)
            && id.is_process()
        {
            let pid = pid.or_else(|| id.pid());
            if let Some(idx) = self.agents.iter().position(|a| a.id == id) {
                self.agents.remove(idx);
                self.mark_agents_dirty();
                self.clamp_selection();
                return;
            }
            if let Some(pid) = pid {
                let indices: Vec<usize> = self
                    .agents
                    .iter()
                    .enumerate()
                    .filter(|(_, a)| a.id.tool() == id.tool() && a.pid == Some(pid))
                    .map(|(idx, _)| idx)
                    .collect();
                for idx in indices.into_iter().rev() {
                    self.agents.remove(idx);
                }
                self.mark_agents_dirty();
                self.clamp_selection();
            }
            return;
        }

        if id.tool() == AgentTool::Codex && id.is_process() {
            let pid = pid.or_else(|| id.pid());

            if let Some(pid) = pid {
                let matching_indices: Vec<usize> = self
                    .agents
                    .iter()
                    .enumerate()
                    .filter(|(_, a)| a.id.tool() == AgentTool::Codex && a.pid == Some(pid))
                    .map(|(idx, _)| idx)
                    .collect();

                if !matching_indices.is_empty() {
                    for agent_idx in matching_indices.into_iter().rev() {
                        if self.agents[agent_idx].id.is_process() {
                            self.agents.remove(agent_idx);
                        } else {
                            let agent = &mut self.agents[agent_idx];
                            agent.status = PrimaryStatus::Exited;
                            agent.activity = None;
                            agent.last_activity = now;
                            push_event(
                                &mut agent.recent_events,
                                ActivityEventKind::StatusChanged,
                                "Process exited".to_string(),
                                now,
                            );
                        }
                    }
                    self.mark_agents_dirty();
                    self.clamp_selection();
                    return;
                }
            }
        }

        if let Some(agent) = self.find_agent_mut_by_id(&id) {
            Self::mark_agent_exited(agent, pid, now);
            self.mark_agents_dirty();
        } else if let Some(pid) = pid {
            if id.tool() == AgentTool::Codex {
                let matching_indices: Vec<usize> = self
                    .agents
                    .iter()
                    .enumerate()
                    .filter(|(_, a)| a.id.tool() == id.tool() && a.pid == Some(pid))
                    .map(|(idx, _)| idx)
                    .collect();

                if !matching_indices.is_empty() {
                    for agent_idx in matching_indices {
                        Self::mark_agent_exited(&mut self.agents[agent_idx], Some(pid), now);
                    }
                    self.mark_agents_dirty();
                    self.clamp_selection();
                    return;
                }
            }

            if let Some(agent) = self.find_agent_mut_by_pid(id.tool(), pid) {
                Self::mark_agent_exited(agent, Some(pid), now);
                self.mark_agents_dirty();
            }
        }
        self.clamp_selection();
    }

    fn handle_session_info(
        &mut self,
        id: AgentId,
        update: SessionInfoUpdate,
        now: chrono::DateTime<chrono::Local>,
    ) {
        let pid = update.pid.or_else(|| id.pid());
        if id.is_session() {
            if let Some(agent) = self.find_agent_mut_by_id(&id) {
                let absorbed_pid = {
                    Self::apply_session_info(
                        agent,
                        pid,
                        update.session_id.clone(),
                        update.branch.clone(),
                        update.is_worktree,
                        update.started_at,
                        now,
                    );
                    agent.pid
                };
                self.mark_agents_dirty();
                self.remove_redundant_codex_process_agents(absorbed_pid);
                self.clamp_selection();
                return;
            }

            if let Some(pid) = pid {
                if let Some(agent) = self.find_process_agent_mut(id.tool(), pid) {
                    agent.id = id.clone();
                    Self::apply_session_info(
                        agent,
                        Some(pid),
                        update.session_id.clone(),
                        update.branch.clone(),
                        update.is_worktree,
                        update.started_at,
                        now,
                    );
                    self.mark_agents_dirty();
                    self.remove_redundant_codex_process_agents(Some(pid));
                    self.clamp_selection();
                    return;
                }
            }

            let agent = self.insert_placeholder_agent(id, pid, now);
            Self::apply_session_info(
                agent,
                pid,
                update.session_id,
                update.branch,
                update.is_worktree,
                update.started_at,
                now,
            );
            self.mark_agents_dirty();
            self.remove_redundant_codex_process_agents(pid);
            self.clamp_selection();
            return;
        }

        if let Some(agent) = self.find_agent_mut_by_id(&id) {
            Self::apply_session_info(
                agent,
                pid,
                update.session_id.clone(),
                update.branch.clone(),
                update.is_worktree,
                update.started_at,
                now,
            );
        } else {
            let agent = self.insert_placeholder_agent(id, pid, now);
            Self::apply_session_info(
                agent,
                pid,
                update.session_id,
                update.branch,
                update.is_worktree,
                update.started_at,
                now,
            );
        }
        self.mark_agents_dirty();
        self.clamp_selection();
    }

    fn handle_todos_updated(
        &mut self,
        id: AgentId,
        pid: Option<u32>,
        todos: Vec<TodoItem>,
        now: chrono::DateTime<chrono::Local>,
    ) {
        if let Some(agent) = self.find_agent_mut_by_id(&id) {
            agent.pid = pid.or(agent.pid);
            agent.todos = todos;
            agent.last_activity = now;
            self.mark_agents_dirty();
        }
    }

    fn mark_agent_exited(
        agent: &mut Agent,
        pid: Option<u32>,
        now: chrono::DateTime<chrono::Local>,
    ) {
        agent.pid = pid.or(agent.pid);
        agent.status = PrimaryStatus::Exited;
        agent.activity = None;
        agent.last_activity = now;
        push_event(
            &mut agent.recent_events,
            ActivityEventKind::StatusChanged,
            "Process exited".to_string(),
            now,
        );
    }

    fn visible_agent_count(&self) -> usize {
        self.with_visible_agent_indices(|indices| indices.len())
    }

    fn with_visible_agent_indices<R>(&self, f: impl FnOnce(&[usize]) -> R) -> R {
        self.refresh_visible_agent_cache();
        let cache = self.visible_agent_cache.borrow();
        f(&cache.indices)
    }

    fn refresh_visible_agent_cache(&self) {
        let current_version = self.agents_version.get();
        let current_filter = self.filter.clone();
        let current_sort_mode = self.sort_mode;
        let should_refresh = {
            let cache = self.visible_agent_cache.borrow();
            cache.version != current_version
                || cache.filter != current_filter
                || cache.sort_mode != Some(current_sort_mode)
        };

        if !should_refresh {
            return;
        }

        let indices = Self::compute_visible_agent_indices(
            &self.agents,
            current_filter.as_deref(),
            current_sort_mode,
        );
        *self.visible_agent_cache.borrow_mut() = VisibleAgentCache {
            version: current_version,
            filter: current_filter,
            sort_mode: Some(current_sort_mode),
            indices,
        };
    }

    fn compute_visible_agent_indices(
        agents: &[Agent],
        filter: Option<&str>,
        sort_mode: SortMode,
    ) -> Vec<usize> {
        let mut indices: Vec<usize> = agents
            .iter()
            .enumerate()
            .filter(|(_, agent)| {
                !(agent.id.tool() == AgentTool::Codex
                    && agent.id.is_session()
                    && agent.pid.is_none())
            })
            .map(|(idx, _)| idx)
            .collect();

        if let Some(filter) = filter {
            let filter = filter.to_lowercase();
            indices.retain(|&idx| Self::agent_matches_filter(&agents[idx], &filter));
        }

        match sort_mode {
            SortMode::Urgency => {
                indices.sort_by_key(|&idx| {
                    let agent = &agents[idx];
                    (Reverse(agent.normalized.status), agent.status)
                });
            }
            SortMode::Name => {
                indices.sort_by(|&left, &right| {
                    let a = &agents[left];
                    let b = &agents[right];
                    a.display_name()
                        .cmp(b.display_name())
                        .then_with(|| a.pid.cmp(&b.pid))
                        .then_with(|| a.id.session_id().cmp(&b.id.session_id()))
                });
            }
            SortMode::Elapsed => {
                indices.sort_by(|&left, &right| {
                    agents[left].started_at.cmp(&agents[right].started_at)
                });
            }
        }

        indices
    }

    fn agent_matches_filter(agent: &Agent, filter: &str) -> bool {
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

    fn mark_agents_dirty(&self) {
        self.agents_version.set(self.agents_version.get() + 1);
    }

    fn apply_session_info(
        agent: &mut Agent,
        pid: Option<u32>,
        session_id: String,
        branch: Option<String>,
        is_worktree: bool,
        started_at: Option<chrono::DateTime<chrono::Local>>,
        now: chrono::DateTime<chrono::Local>,
    ) {
        agent.pid = pid.or(agent.pid);
        if !session_id.is_empty() {
            agent.session_id = Some(session_id);
        }
        if branch.is_some() {
            agent.branch = branch;
        }
        agent.is_worktree = is_worktree;
        if let Some(real_start) = started_at {
            if real_start < agent.started_at {
                agent.started_at = real_start;
            }
        }
        agent.last_activity = now;
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
        assert_eq!(list[0], AccountEntry { provider: Provider::Codex, profile_id: "c1".to_string() });
        assert_eq!(list[1], AccountEntry { provider: Provider::Codex, profile_id: "c2".to_string() });
        assert_eq!(list[2], AccountEntry { provider: Provider::Claude, profile_id: "cl1".to_string() });
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
}
