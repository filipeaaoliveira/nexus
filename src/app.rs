use std::cell::{Cell, RefCell};
use std::cmp::Reverse;
use std::collections::VecDeque;
use std::fs;
use std::path::PathBuf;

use anyhow::{anyhow, bail, Context, Result};
use chrono::Local;
use chrono::Utc;
use serde_json::Value;

use crate::codex_profiles::profile::{
    extract_auth_metadata, load_profile_metadata, save_profile_metadata, CodexProfile,
    ProfileMetadata,
};
use crate::codex_profiles::quota::{
    fetch_quota_from_api_sync, load_quota_cache, parse_quota_from_sessions, save_quota_cache,
    QuotaCache, QuotaSnapshot,
};
use crate::codex_profiles::runtime::{
    get_codex_quota, launch_codex, require_codex_binary, RuntimePlan,
};
use crate::codex_profiles::ProfileAuth;
use crate::config::Config;
use crate::event::AgentEvent;
use crate::types::*;

const MAX_RECENT_EVENTS: usize = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveTab {
    Overview,
    Codex,
}

impl ActiveTab {
    pub const ALL: [Self; 2] = [Self::Overview, Self::Codex];

    pub fn next(self) -> Self {
        match self {
            Self::Overview => Self::Codex,
            Self::Codex => Self::Overview,
        }
    }

    pub fn previous(self) -> Self {
        match self {
            Self::Overview => Self::Codex,
            Self::Codex => Self::Overview,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Overview => "Overview",
            Self::Codex => "Codex",
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
pub struct CodexTabState {
    pub profile_ids: Vec<String>,
    pub selected_profile_index: Option<usize>,
    pub active_profile_id: Option<String>,
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
    pub quota_cache: Option<QuotaCache>,
    pub quota_edit_mode: bool,
    pub quota_five_hour: String,
    pub quota_weekly: String,
    pub quota_edit_field_weekly: bool, // false = 5h, true = weekly
    pub quota_loading: bool,           // true when async fetch is in progress
    pub pending_quota: Option<QuotaSnapshot>, // result from async fetch
}

impl Default for CodexTabState {
    fn default() -> Self {
        Self {
            profile_ids: Vec::new(),
            selected_profile_index: None,
            active_profile_id: None,
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
            quota_cache: None,
            quota_edit_mode: false,
            quota_five_hour: String::new(),
            quota_weekly: String::new(),
            quota_edit_field_weekly: false,
            quota_loading: false,
            pending_quota: None,
        }
    }
}

impl CodexTabState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn next_profile(&mut self) {
        let len = self.profile_ids.len();
        if len == 0 {
            self.selected_profile_index = None;
            self.active_profile_id = None;
            self.quota = None;
            return;
        }

        let new_index = match self.selected_profile_index {
            Some(index) => (index + 1) % len,
            None => 0,
        };
        self.selected_profile_index = Some(new_index);
        self.active_profile_id = Some(self.profile_ids[new_index].clone());
        self.load_quota_from_cache();
    }

    pub fn previous_profile(&mut self) {
        let len = self.profile_ids.len();
        if len == 0 {
            self.selected_profile_index = None;
            self.active_profile_id = None;
            self.quota = None;
            return;
        }

        let new_index = match self.selected_profile_index {
            Some(index) => (index + len - 1) % len,
            None => len - 1,
        };
        self.selected_profile_index = Some(new_index);
        self.active_profile_id = Some(self.profile_ids[new_index].clone());
        self.load_quota_from_cache();
    }

    pub fn set_active(&mut self, profile_id: Option<String>) {
        self.active_profile_id = profile_id;
    }

    /// Load quota from cache for currently selected profile
    pub fn load_quota_from_cache(&mut self) {
        if let Some(profile_id) = self
            .selected_profile_index
            .and_then(|i| self.profile_ids.get(i).cloned())
        {
            if let Some(ref cache) = self.quota_cache {
                if let Some(quota) = cache.by_profile.get(&profile_id) {
                    self.quota = Some(quota.clone());
                }
            }
        }
    }

    /// Apply any pending quota result from async fetch
    pub fn apply_pending_quota(&mut self, profile_id: &str, quota: QuotaSnapshot) {
        let is_selected = self
            .selected_profile_index
            .and_then(|i| self.profile_ids.get(i))
            .map(|id| id == profile_id)
            .unwrap_or(false);

        if is_selected {
            self.quota = Some(quota);
        }
        self.quota_loading = false;
        self.pending_quota = None;
    }

    /// Check if a profile is currently selected
    pub fn is_profile_selected(&self, profile_id: &str) -> bool {
        self.selected_profile_index
            .and_then(|i| self.profile_ids.get(i))
            .map(|id| id == profile_id)
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
    pub codex_tab: CodexTabState,
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
}

impl AppState {
    pub fn new(config: Config) -> Self {
        let mut codex_tab = CodexTabState::default();

        // Load cached quota from file
        if let Ok(cache) = load_quota_cache(&config.codex_quota_cache_file) {
            codex_tab.quota_cache = Some(cache);
        }

        let mut state = Self {
            active_tab: ActiveTab::Overview,
            codex_tab,
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
        };
        state.reload_codex_profiles();
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
            ActiveTab::Codex => self.codex_tab.filter_mode,
        }
    }

    pub fn active_rename_mode(&self) -> bool {
        match self.active_tab {
            ActiveTab::Overview => self.rename_mode,
            ActiveTab::Codex => false,
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
            AgentEvent::CodexQuotaRefreshed { profile_id, quota } => {
                if let Some(selected_id) = self.selected_codex_profile_id() {
                    if selected_id == profile_id {
                        self.codex_tab.quota = quota;
                    }
                }
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
        self.codex_tab.next_profile();
    }

    pub fn handle_codex_move_up(&mut self) {
        self.codex_tab.previous_profile();
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
        self.codex_tab.filter_mode = false;
        self.codex_tab.filter_input.clear();
    }

    pub fn start_filter(&mut self) {
        self.filter_mode = true;
        self.filter_input.clear();
        self.filter = None;
    }

    pub fn start_codex_filter(&mut self) {
        self.codex_tab.filter_mode = true;
        self.codex_tab.filter_input.clear();
    }

    pub fn start_session_picker(&mut self) {
        self.codex_tab.session_picker_mode = true;
        self.codex_tab.session_picker_selected = self.codex_tab.selected_profile_index;
    }

    pub fn dismiss_session_picker(&mut self) {
        self.codex_tab.session_picker_mode = false;
        self.codex_tab.session_picker_selected = None;
        self.codex_tab.session_picker_sessions = None;
    }

    pub fn session_picker_move_up(&mut self) {
        let is_profiles = self.codex_tab.session_picker_sessions.is_none();
        let items: &[String] = if is_profiles {
            &self.codex_tab.profile_ids
        } else {
            self.codex_tab.session_picker_sessions.as_ref().unwrap()
        };

        let len = items.len();
        if len == 0 {
            self.codex_tab.session_picker_selected = None;
            return;
        }
        let new_index = match self.codex_tab.session_picker_selected {
            Some(index) => (index + len - 1) % len,
            None => len - 1,
        };
        self.codex_tab.session_picker_selected = Some(new_index);
    }

    pub fn session_picker_move_down(&mut self) {
        let is_profiles = self.codex_tab.session_picker_sessions.is_none();
        let items: &[String] = if is_profiles {
            &self.codex_tab.profile_ids
        } else {
            self.codex_tab.session_picker_sessions.as_ref().unwrap()
        };

        let len = items.len();
        if len == 0 {
            self.codex_tab.session_picker_selected = None;
            return;
        }
        let new_index = match self.codex_tab.session_picker_selected {
            Some(index) => (index + 1) % len,
            None => 0,
        };
        self.codex_tab.session_picker_selected = Some(new_index);
    }

    pub fn confirm_session_picker(&mut self) {
        if let Some(index) = self.codex_tab.session_picker_selected {
            if self.codex_tab.session_picker_sessions.is_some() {
                // Confirming session resume - extract session_id first
                let session_id = self
                    .codex_tab
                    .session_picker_sessions
                    .as_ref()
                    .and_then(|sessions| sessions.get(index).map(|s| s.to_string()));

                self.dismiss_session_picker();

                if let Some(sid) = session_id {
                    self.resume_codex_session(&sid);
                }
            } else {
                // Confirming profile selection
                self.codex_tab.selected_profile_index = Some(index);
                self.codex_tab.active_profile_id = self.codex_tab.profile_ids.get(index).cloned();
                self.codex_tab.view_mode = true;
                self.dismiss_session_picker();
                self.codex_tab.load_quota_from_cache();
            }
        } else {
            self.dismiss_session_picker();
        }
    }

    pub fn get_repo_sessions(&self) -> Vec<String> {
        let repo_dir = self.config.codex_repos_dir.join("current-repo");
        let sessions_dir = repo_dir.join("sessions");

        if !sessions_dir.is_dir() {
            return Vec::new();
        }

        let mut sessions = Vec::new();

        // Read year directories (e.g., 2025, 2026)
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

                // Read month directories (e.g., 01, 02, 03)
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

                        // Read actual session directories
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
                                // Format: 2026/04/04
                                sessions
                                    .push(format!("{}/{}/{}", year_name, month_name, session_name));
                            }
                        }
                    }
                }
            }
        }

        sessions.sort();
        sessions
    }

    pub fn start_session_picker_for_resume(&mut self) {
        let sessions = self.get_repo_sessions();
        if sessions.is_empty() {
            self.codex_tab.status_message = Some("No sessions found in current repo".to_string());
            return;
        }

        self.codex_tab.session_picker_mode = true;
        self.codex_tab.session_picker_sessions = Some(sessions);
        self.codex_tab.session_picker_selected = None;
    }

    pub fn start_codex_edit(&mut self) {
        let profile_id = match self.selected_codex_profile_id() {
            Some(id) => id.to_string(),
            None => return,
        };
        let profile_dir = self.config.codex_profiles_dir.join(&profile_id);

        let mut edit_email = None;
        let mut edit_plan = None;
        let auth_path = profile_dir.join("auth.json");
        if let Ok(auth_content) = fs::read_to_string(&auth_path) {
            if let Ok(auth_json) = serde_json::from_str::<Value>(&auth_content) {
                let auth_meta = extract_auth_metadata(&auth_json);
                edit_email = auth_meta.email;
                edit_plan = auth_meta.plan;
            }
        }

        if let Ok(metadata) = load_profile_metadata(&profile_dir) {
            self.codex_tab.start_edit(
                metadata.label.as_deref().unwrap_or(&profile_id),
                edit_email.as_deref(),
                edit_plan.as_deref(),
                metadata.notes.as_deref().unwrap_or(""),
                metadata.enabled,
            );
            return;
        }
        self.codex_tab.start_edit(
            &profile_id,
            edit_email.as_deref(),
            edit_plan.as_deref(),
            "",
            true,
        );
    }

    pub fn confirm_codex_edit(&mut self) -> Result<()> {
        if !self.codex_tab.edit_mode {
            return Ok(());
        }
        let profile_id = match self.selected_codex_profile_id() {
            Some(id) => id.to_string(),
            None => return Err(anyhow!("no profile selected")),
        };
        let profile_dir = self.config.codex_profiles_dir.join(&profile_id);

        let metadata = ProfileMetadata {
            id: profile_id.clone(),
            label: Some(self.codex_tab.edit_label.clone()),
            email: None,
            plan: None,
            notes: if self.codex_tab.edit_notes.is_empty() {
                None
            } else {
                Some(self.codex_tab.edit_notes.clone())
            },
            enabled: self.codex_tab.edit_enabled,
            created_at: None,
            updated_at: Some(Utc::now()),
        };

        save_profile_metadata(&profile_dir, &metadata)?;
        self.codex_tab.edit_mode = false;
        self.codex_tab.status_message = Some(format!("Saved profile {}", profile_id));
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

        if let Some(active) = &self.codex_tab.active_profile_id {
            if active == &profile_id {
                self.codex_tab.active_profile_id = None;
            }
        }

        self.codex_tab.profile_ids.retain(|id| id != &profile_id);
        self.codex_tab.selected_profile_index = None;
        self.codex_tab.view_mode = false;
        self.codex_tab.status_message = Some(format!("Deleted profile {}", profile_id));
        Ok(())
    }

    pub fn cancel_codex_edit(&mut self) {
        self.codex_tab.cancel_edit();
    }

    pub fn refresh_codex_quota(&mut self) {
        let profile_id = match self.selected_codex_profile_id() {
            Some(id) => id.to_string(),
            None => {
                self.codex_tab.quota = None;
                return;
            }
        };

        tracing::debug!("refreshing quota for profile: {}", profile_id);

        // First try API for the selected profile
        let profile_auth_path = self
            .config
            .codex_profiles_dir
            .join(&profile_id)
            .join("auth.json");

        let api_quota = if profile_auth_path.exists() {
            match ProfileAuth::load(&profile_auth_path) {
                Ok(auth) => fetch_quota_from_api_sync(&auth),
                Err(_) => None,
            }
        } else {
            None
        };

        // If API succeeded, use it
        let quota = api_quota.or_else(|| {
            // Fall back to sessions-based parse
            let sessions_dir = &self.config.codex_sessions_dir;
            parse_quota_from_sessions(sessions_dir).or_else(|| {
                // Fall back to CLI-based parse
                let profile_dir = self.config.codex_profiles_dir.join(&profile_id);
                let runtime_home = self.config.codex_runtime_dir.join(&profile_id);
                let repo_dir = self.config.codex_repos_dir.join("current-repo");
                let runtime = RuntimePlan::new(profile_dir, repo_dir, runtime_home);

                let codex_bin = require_codex_binary(self.config.codex_binary.as_deref())
                    .map_err(|e| {
                        tracing::debug!("codex binary not found: {}", e);
                    })
                    .ok()?;

                match get_codex_quota(&codex_bin, &runtime) {
                    Ok(Some(q)) => Some(q),
                    Ok(None) => {
                        tracing::debug!("no quota returned from codex status");
                        None
                    }
                    Err(e) => {
                        tracing::debug!("failed to get codex quota: {}", e);
                        None
                    }
                }
            })
        });

        if let Some(quota) = quota {
            let mut cache = self.codex_tab.quota_cache.take().unwrap_or_default();
            cache.by_profile.insert(profile_id.clone(), quota.clone());
            if let Err(e) = save_quota_cache(&cache, &self.config.codex_quota_cache_file) {
                tracing::warn!("failed to save quota cache: {}", e);
            } else {
                self.codex_tab.quota_cache = Some(cache);
            }
            self.codex_tab.quota = Some(quota);
        } else {
            self.codex_tab.quota = None;
        }
    }

    pub fn refresh_all_codex_quotas(&mut self) {
        let profile_ids = self.codex_tab.profile_ids.clone();
        let mut all_refreshed: Vec<(String, QuotaSnapshot)> = Vec::new();

        for profile_id in &profile_ids {
            // Try API for each profile
            let profile_auth_path = self
                .config
                .codex_profiles_dir
                .join(profile_id)
                .join("auth.json");

            let api_quota = if profile_auth_path.exists() {
                match ProfileAuth::load(&profile_auth_path) {
                    Ok(auth) => fetch_quota_from_api_sync(&auth),
                    Err(_) => None,
                }
            } else {
                None
            };

            if let Some(quota) = api_quota {
                all_refreshed.push((profile_id.clone(), quota));
            } else {
                // Fall back to sessions for this profile
                let sessions_dir = &self.config.codex_sessions_dir;
                if let Some(snap) = parse_quota_from_sessions(sessions_dir) {
                    all_refreshed.push((profile_id.clone(), snap));
                }
            }
        }

        // Update cache with all refreshed quotas
        let mut cache = self.codex_tab.quota_cache.take().unwrap_or_default();
        for (profile_id, quota) in all_refreshed {
            cache.by_profile.insert(profile_id, quota);
        }
        if let Err(e) = save_quota_cache(&cache, &self.config.codex_quota_cache_file) {
            tracing::warn!("failed to save quota cache: {}", e);
        } else {
            self.codex_tab.quota_cache = Some(cache);
        }

        // If there's a selected profile, update its current quota display
        if let Some(selected_id) = self.selected_codex_profile_id() {
            if let Some(ref cache) = self.codex_tab.quota_cache {
                self.codex_tab.quota = cache.by_profile.get(selected_id).cloned();
            }
        }
    }

    pub fn start_codex_quota_edit(&mut self) {
        let _profile_id = match self.selected_codex_profile_id() {
            Some(id) => id,
            None => return,
        };

        // Initialize with current quota values or defaults
        if let Some(ref quota) = self.codex_tab.quota {
            self.codex_tab.quota_five_hour = match quota.five_hour_state {
                crate::codex_profiles::quota::LimitState::Hit => "HIT".to_string(),
                crate::codex_profiles::quota::LimitState::Available => "ok".to_string(),
                crate::codex_profiles::quota::LimitState::Unknown => "?".to_string(),
            };
            self.codex_tab.quota_weekly = match quota.weekly_state {
                crate::codex_profiles::quota::LimitState::Hit => "HIT".to_string(),
                crate::codex_profiles::quota::LimitState::Available => "ok".to_string(),
                crate::codex_profiles::quota::LimitState::Unknown => "?".to_string(),
            };
        } else {
            self.codex_tab.quota_five_hour = "ok".to_string();
            self.codex_tab.quota_weekly = "ok".to_string();
        }
        self.codex_tab.quota_edit_mode = true;
    }

    pub fn dismiss_codex_quota_edit(&mut self) {
        self.codex_tab.quota_edit_mode = false;
        self.codex_tab.quota_edit_field_weekly = false;
    }

    pub fn codex_quota_edit_push_char(&mut self, _c: char) {
        // Space/enter cycles; this handles freeform input by cycling on any char
        self.cycle_quota_edit_field();
    }

    fn cycle_quota_edit_field(&mut self) {
        fn cycle(val: &str) -> &'static str {
            match val.to_uppercase().as_str() {
                "OK" | "ok" => "HIT",
                "HIT" => "?",
                _ => "ok",
            }
        }
        if self.codex_tab.quota_edit_field_weekly {
            let next = cycle(&self.codex_tab.quota_weekly).to_string();
            self.codex_tab.quota_weekly = next;
        } else {
            let next = cycle(&self.codex_tab.quota_five_hour).to_string();
            self.codex_tab.quota_five_hour = next;
        }
    }

    pub fn codex_quota_edit_pop_char(&mut self) {
        // Backspace also cycles (same as space)
        self.cycle_quota_edit_field();
    }

    pub fn codex_quota_edit_next_field(&mut self) {
        self.codex_tab.quota_edit_field_weekly = !self.codex_tab.quota_edit_field_weekly;
    }

    pub fn codex_quota_edit_prev_field(&mut self) {
        self.codex_tab.quota_edit_field_weekly = !self.codex_tab.quota_edit_field_weekly;
    }

    pub fn confirm_codex_quota_edit(&mut self) -> Result<()> {
        let profile_id = match self.selected_codex_profile_id() {
            Some(id) => id.to_string(),
            None => return Err(anyhow!("no profile selected")),
        };

        // Parse the input strings to create a QuotaSnapshot
        let five_hour_state = match self.codex_tab.quota_five_hour.to_uppercase().as_str() {
            "HIT" => crate::codex_profiles::quota::LimitState::Hit,
            "?" => crate::codex_profiles::quota::LimitState::Unknown,
            _ => crate::codex_profiles::quota::LimitState::Available,
        };

        let weekly_state = match self.codex_tab.quota_weekly.to_uppercase().as_str() {
            "HIT" => crate::codex_profiles::quota::LimitState::Hit,
            "?" => crate::codex_profiles::quota::LimitState::Unknown,
            _ => crate::codex_profiles::quota::LimitState::Available,
        };

        let summary = format!("5h={:?}; weekly={:?}", five_hour_state, weekly_state);

        let quota = QuotaSnapshot {
            captured_at: Utc::now(),
            confidence: crate::codex_profiles::quota::Confidence::Exact,
            five_hour_state,
            five_hour_reset_at: None,
            five_hour_used_pct: None,
            weekly_state,
            weekly_reset_at: None,
            weekly_used_pct: None,
            summary,
        };

        // Save to cache
        let mut cache = self.codex_tab.quota_cache.take().unwrap_or_default();
        cache.by_profile.insert(profile_id.clone(), quota.clone());
        if let Err(e) = save_quota_cache(&cache, &self.config.codex_quota_cache_file) {
            tracing::warn!("failed to save quota cache: {}", e);
        } else {
            self.codex_tab.quota_cache = Some(cache);
        }

        self.codex_tab.quota = Some(quota);
        self.codex_tab.quota_edit_mode = false;
        self.codex_tab.status_message = Some(format!("Saved quota for {}", profile_id));
        Ok(())
    }

    pub fn codex_edit_push_char(&mut self, c: char) {
        match self.codex_tab.edit_field {
            EditField::Label => self.codex_tab.edit_label.push(c),
            EditField::Notes => self.codex_tab.edit_notes.push(c),
            EditField::Enabled => {}
        }
    }

    pub fn codex_edit_pop_char(&mut self) {
        match self.codex_tab.edit_field {
            EditField::Label => {
                self.codex_tab.edit_label.pop();
            }
            EditField::Notes => {
                self.codex_tab.edit_notes.pop();
            }
            EditField::Enabled => {}
        }
    }

    pub fn codex_edit_toggle_enabled(&mut self) {
        self.codex_tab.edit_enabled = !self.codex_tab.edit_enabled;
    }

    pub fn codex_edit_next_field(&mut self) {
        self.codex_tab.edit_field = EditField::Notes;
    }

    pub fn codex_edit_prev_field(&mut self) {
        self.codex_tab.edit_field = EditField::Label;
    }

    pub fn dismiss_codex_edit(&mut self) {
        self.codex_tab.cancel_edit();
    }

    pub fn filter_push_char(&mut self, c: char) {
        self.filter_input.push(c);
        self.sync_filter();
    }

    pub fn codex_filter_push_char(&mut self, c: char) {
        self.codex_tab.filter_input.push(c);
    }

    pub fn filter_pop_char(&mut self) {
        self.filter_input.pop();
        self.sync_filter();
    }

    pub fn codex_filter_pop_char(&mut self) {
        self.codex_tab.filter_input.pop();
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
        self.codex_tab.filter_mode = false;
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

        let next_index = self.codex_tab.profile_ids.len() + 1;
        let profile_id = format!("profile-{next_index:02}");
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
        self.codex_tab.status_message = Some(format!("Imported {}", profile.id));
        Ok(())
    }

    pub fn launch_selected_codex_profile(&mut self) -> Result<()> {
        let profile_id = self
            .selected_codex_profile_id()
            .ok_or_else(|| anyhow!("no Codex profile selected"))?;
        let profile_dir = self.config.codex_profiles_dir.join(profile_id);
        let repo_dir = self.config.codex_repos_dir.join("current-repo");
        let runtime_home = self.config.codex_runtime_dir.join(profile_id);
        let working_dir =
            std::env::current_dir().context("failed to resolve current working directory")?;
        let runtime = RuntimePlan::new(profile_dir, repo_dir, runtime_home);
        let codex_bin = require_codex_binary(self.config.codex_binary.as_deref())?;

        let status = launch_codex(
            codex_bin.as_path(),
            &runtime,
            &self.config.codex_launch_args,
            working_dir.as_path(),
        )?;

        self.codex_tab.status_message = Some(if status.success() {
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
                self.codex_tab.status_message = Some("No profile selected".to_string());
                return;
            }
        };

        let profile_dir = self.config.codex_profiles_dir.join(&profile_id);
        let repo_dir = self.config.codex_repos_dir.join("current-repo");
        let runtime_home = self.config.codex_runtime_dir.join(&profile_id);
        let working_dir = match std::env::current_dir() {
            Ok(d) => d,
            Err(e) => {
                self.codex_tab.status_message = Some(format!("failed to resolve cwd: {}", e));
                return;
            }
        };

        let runtime = RuntimePlan::new(profile_dir, repo_dir, runtime_home);

        let codex_bin = match require_codex_binary(self.config.codex_binary.as_deref()) {
            Ok(b) => b,
            Err(e) => {
                self.codex_tab.status_message = Some(format!("failed to find codex: {}", e));
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
                self.codex_tab.status_message = Some(if status.success() {
                    format!("Resumed session {} with {}", session_id, profile_id)
                } else {
                    format!("Session exited with status {} for {}", status, profile_id)
                });
            }
            Err(e) => {
                self.codex_tab.status_message = Some(format!("Failed to resume: {}", e));
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

        // Run directory scan in spawn_blocking to avoid blocking
        let profile_ids = std::thread::spawn(move || {
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
        })
        .join()
        .unwrap_or_default();

        self.codex_tab.profile_ids = profile_ids;

        // After setting profile IDs, select and load quota from cache
        if let Some(profile_id) = selected_profile_id.as_deref() {
            self.select_codex_profile_id(profile_id);
        } else {
            self.clamp_codex_selection();
            // Try to load quota from cache for selected profile
            self.load_quota_from_cache();
        }
    }

    /// Load quota from cache for currently selected profile
    pub fn load_quota_from_cache(&mut self) {
        if let Some(profile_id) = self.selected_codex_profile_id() {
            if let Some(ref cache) = self.codex_tab.quota_cache {
                if let Some(quota) = cache.by_profile.get(profile_id) {
                    self.codex_tab.quota = Some(quota.clone());
                }
            }
        }
    }

    pub fn selected_codex_profile_id(&self) -> Option<&str> {
        let index = self.codex_tab.selected_profile_index?;
        self.codex_tab.profile_ids.get(index).map(String::as_str)
    }

    pub fn update_quota_for_profile(&mut self, profile_id: &str, quota: QuotaSnapshot) {
        // Update cache
        let mut cache = self.codex_tab.quota_cache.take().unwrap_or_default();
        cache
            .by_profile
            .insert(profile_id.to_string(), quota.clone());
        if let Err(e) = save_quota_cache(&cache, &self.config.codex_quota_cache_file) {
            tracing::warn!("failed to save quota cache: {}", e);
        } else {
            self.codex_tab.quota_cache = Some(cache);
        }

        // Update current display if this is the selected profile
        if self.selected_codex_profile_id() == Some(profile_id) {
            self.codex_tab.quota = Some(quota);
        }
    }

    pub fn set_active_codex_profile(&mut self, profile_id: Option<String>) {
        self.codex_tab.set_active(profile_id);
    }

    pub fn active_codex_profile_id(&self) -> Option<&str> {
        self.codex_tab.active_profile_id.as_deref()
    }

    pub fn write_active_profile_to_codex_home(&mut self) -> Result<()> {
        let active_profile_id = self
            .active_codex_profile_id()
            .ok_or_else(|| anyhow!("no active profile"))?;

        let profile_dir = self.config.codex_profiles_dir.join(active_profile_id);
        let source_auth = profile_dir.join("auth.json");

        if !source_auth.is_file() {
            bail!("profile auth.json not found at {}", source_auth.display());
        }

        let home = dirs::home_dir().ok_or_else(|| anyhow!("home directory not found"))?;
        let target_codex = home.join(".codex");
        let target_auth = target_codex.join("auth.json");

        fs::create_dir_all(&target_codex)?;
        fs::copy(&source_auth, &target_auth)?;

        self.codex_tab.status_message =
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

    pub fn restart_running_codex_with_profile(&mut self) -> Result<usize> {
        let active_profile_id = match self.active_codex_profile_id() {
            Some(id) => id,
            None => return Ok(0),
        };

        let running_sessions = self.running_codex_sessions();
        if running_sessions.is_empty() {
            return Ok(0);
        }

        let profile_dir = self.config.codex_profiles_dir.join(active_profile_id);
        let repo_dir = self.config.codex_repos_dir.join("current-repo");
        let runtime_home = self.config.codex_runtime_dir.join(active_profile_id);
        let working_dir =
            std::env::current_dir().context("failed to resolve current working directory")?;
        let runtime = RuntimePlan::new(profile_dir, repo_dir, runtime_home);
        let codex_bin = require_codex_binary(self.config.codex_binary.as_deref())?;

        let mut restarted = 0;
        for (session_id, _pid) in &running_sessions {
            let args = vec!["resume".to_string(), session_id.clone()];
            let status = launch_codex(codex_bin.as_path(), &runtime, &args, working_dir.as_path())?;

            if status.success() {
                restarted += 1;
            }
        }

        self.codex_tab.status_message = Some(format!(
            "Restarted {} session(s) with {}",
            restarted, active_profile_id
        ));
        Ok(restarted)
    }

    fn select_codex_profile_id(&mut self, profile_id: &str) {
        self.codex_tab.selected_profile_index = self
            .codex_tab
            .profile_ids
            .iter()
            .position(|candidate| candidate == profile_id);
        self.clamp_codex_selection();
        // Quota will be loaded via background fetcher and cached in memory
    }

    fn clamp_codex_selection(&mut self) {
        let len = self.codex_tab.profile_ids.len();
        self.codex_tab.selected_profile_index = match (self.codex_tab.selected_profile_index, len) {
            (_, 0) => None,
            (Some(index), len) if index < len => Some(index),
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
        cwd: &PathBuf,
    ) -> Vec<usize> {
        self.agents
            .iter()
            .enumerate()
            .filter(|(_, a)| {
                a.id.tool() == tool
                    && a.id.is_session()
                    && a.pid == Some(pid)
                    && a.status != PrimaryStatus::Exited
                    && (a.cwd.as_os_str().is_empty() || a.cwd == *cwd)
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
        if agent.id.tool() == AgentTool::Codex && agent.id.is_session() {
            agent.pid = pid;
        } else {
            agent.pid = pid.or(agent.pid);
        }
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
    use super::AppState;
    use crate::codex_profiles::profile::{
        load_profile_metadata, save_profile_metadata, CodexProfile, ProfileMetadata,
    };
    use crate::config::Config;
    use crate::event::AgentEvent;
    use crate::types::{AgentId, AgentTool, PrimaryStatus, SortMode};
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

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
}
