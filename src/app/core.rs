use chrono::Local;

use super::*;
use crate::event::AgentEvent;

pub struct AppState {
    pub(in crate::app) root: RootState,
    pub(in crate::app) agents: Vec<Agent>,
}

#[cfg(test)]
impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

impl AppState {
    #[cfg(test)]
    pub fn new() -> Self {
        Self::from_root(RootState::default())
    }

    pub fn from_root(root: RootState) -> Self {
        Self {
            root,
            agents: Vec::new(),
        }
    }

    pub fn active_tab(&self) -> Workspace {
        self.root.workspace().active()
    }

    pub fn root(&self) -> &RootState {
        &self.root
    }

    pub fn agents(&self) -> &[Agent] {
        &self.agents
    }

    #[cfg(test)]
    pub fn next_tab(&mut self) {
        self.root.next_workspace();
    }

    #[cfg(test)]
    pub fn previous_tab(&mut self) {
        self.root.previous_workspace();
    }

    pub fn show_help(&self) -> bool {
        matches!(self.root.system().overlay(), Overlay::Help { .. })
    }

    pub fn help_scroll(&self) -> u16 {
        match self.root.system().overlay() {
            Overlay::Help { scroll } => *scroll,
            Overlay::None => 0,
        }
    }

    pub fn handle_event(&mut self, event: AgentEvent) {
        let now = Local::now();
        match event {
            AgentEvent::DetectorHealthChanged {
                detector,
                healthy,
                message,
            } => self.root.record_detector_health(detector, healthy, message),
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
                self.handle_normalized_state_changed(id, pid, state, now);
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
        self.clamp_selection();
    }

    pub fn visible_agents(&self) -> Vec<&Agent> {
        crate::app::projection::visible_agents(&self.agents, self.root.overview())
    }

    #[cfg(test)]
    pub fn count_by_status(&self, status: PrimaryStatus) -> usize {
        crate::app::projection::count_by_status(&self.agents, self.root.overview(), status)
    }

    pub fn primary_detector_issue(&self) -> Option<&str> {
        self.root
            .system()
            .detector_health()
            .values()
            .find(|health| !health.healthy)
            .map(|health| health.message.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::state::AccountsState;
    use crate::codex_profiles::profile::{
        load_profile_metadata, save_profile_metadata, ProfileMetadata, StoredCodexProfile,
    };
    use crate::domain::agents::ids::{AgentId, AgentTool};
    use crate::domain::agents::model::SortMode;
    use crate::domain::agents::status::PrimaryStatus;
    use crate::domain::system::DetectorKind;
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
        let mut state = AppState::new();
        state.handle_event(discovered(AgentTool::Claude, 1, "/tmp/claude"));
        state.handle_event(discovered(AgentTool::OpenCode, 2, "/tmp/opencode"));

        assert_eq!(state.visible_agents().len(), 2);

        state.root.overview.filter = Some("claude".to_string());
        assert_eq!(state.visible_agents().len(), 1);
        assert_eq!(state.visible_agents()[0].pid(), Some(1));

        state.root.overview.filter = Some("opencode".to_string());
        assert_eq!(state.visible_agents().len(), 1);
        assert_eq!(state.visible_agents()[0].pid(), Some(2));
    }

    #[test]
    fn visible_agent_indices_refresh_when_sort_changes() {
        let mut state = AppState::new();
        state.handle_event(discovered(AgentTool::OpenCode, 2, "/tmp/opencode"));
        state.handle_event(discovered(AgentTool::Claude, 1, "/tmp/claude"));

        assert_eq!(state.visible_agents()[0].pid(), Some(1));

        state.root.overview.sort_mode = SortMode::Elapsed;
        assert_eq!(state.visible_agents()[0].pid(), Some(2));

        state.handle_event(AgentEvent::StatusChanged {
            id: AgentId::new(AgentTool::OpenCode, 2),
            pid: Some(2),
            old: PrimaryStatus::Running,
            new: PrimaryStatus::Waiting,
        });
        state.root.overview.sort_mode = SortMode::Urgency;
        assert_eq!(state.visible_agents()[0].pid(), Some(2));
    }

    #[test]
    fn visible_agents_projects_from_authoritative_agent_store() {
        let mut state = AppState::new();
        let now = chrono::Local::now();
        state.agents = vec![
            Agent {
                ident: AgentIdent {
                    id: AgentId::new(AgentTool::OpenCode, 2),
                    pid: Some(2),
                    session_id: None,
                    alias: None,
                    cwd: Some(PathBuf::from("/tmp/opencode")),
                    branch: Some("main".into()),
                    is_worktree: false,
                    started_at: now,
                },
                lifecycle: AgentLifecycle {
                    status: PrimaryStatus::Running,
                    normalized: NormalizedState::new(NormalizedStatus::Working, None),
                    activity: None,
                    last_activity: now,
                },
                detail: AgentDetail {
                    subagents: Vec::new(),
                    recent_events: std::collections::VecDeque::new(),
                    todos: Vec::new(),
                },
            },
            Agent {
                ident: AgentIdent {
                    id: AgentId::new(AgentTool::Claude, 1),
                    pid: Some(1),
                    session_id: None,
                    alias: None,
                    cwd: Some(PathBuf::from("/tmp/claude")),
                    branch: Some("main".into()),
                    is_worktree: false,
                    started_at: now,
                },
                lifecycle: AgentLifecycle {
                    status: PrimaryStatus::Running,
                    normalized: NormalizedState::new(NormalizedStatus::Working, None),
                    activity: None,
                    last_activity: now,
                },
                detail: AgentDetail {
                    subagents: Vec::new(),
                    recent_events: std::collections::VecDeque::new(),
                    todos: Vec::new(),
                },
            },
        ];

        let visible = state.visible_agents();
        assert_eq!(visible.len(), 2);
        assert_eq!(*visible[0].id(), AgentId::new(AgentTool::Claude, 1));
        assert_eq!(*visible[1].id(), AgentId::new(AgentTool::OpenCode, 2));
    }

    #[test]
    fn visible_agents_applies_filter_without_stored_overview_rows() {
        let mut state = AppState::new();
        let now = chrono::Local::now();
        state.agents = vec![
            Agent {
                ident: AgentIdent {
                    id: AgentId::new(AgentTool::Claude, 1),
                    pid: Some(1),
                    session_id: None,
                    alias: Some("code-review".into()),
                    cwd: Some(PathBuf::from("/tmp/claude")),
                    branch: Some("main".into()),
                    is_worktree: false,
                    started_at: now,
                },
                lifecycle: AgentLifecycle {
                    status: PrimaryStatus::Running,
                    normalized: NormalizedState::new(NormalizedStatus::Working, None),
                    activity: None,
                    last_activity: now,
                },
                detail: AgentDetail {
                    subagents: Vec::new(),
                    recent_events: std::collections::VecDeque::new(),
                    todos: Vec::new(),
                },
            },
            Agent {
                ident: AgentIdent {
                    id: AgentId::new(AgentTool::OpenCode, 2),
                    pid: Some(2),
                    session_id: None,
                    alias: None,
                    cwd: Some(PathBuf::from("/tmp/opencode")),
                    branch: Some("feature".into()),
                    is_worktree: false,
                    started_at: now,
                },
                lifecycle: AgentLifecycle {
                    status: PrimaryStatus::Running,
                    normalized: NormalizedState::new(NormalizedStatus::NeedsInput, None),
                    activity: None,
                    last_activity: now,
                },
                detail: AgentDetail {
                    subagents: Vec::new(),
                    recent_events: std::collections::VecDeque::new(),
                    todos: Vec::new(),
                },
            },
        ];
        state.root.overview.filter = Some("review".into());

        let visible = state.visible_agents();
        assert_eq!(visible.len(), 1);
        assert_eq!(*visible[0].id(), AgentId::new(AgentTool::Claude, 1));
    }

    /// Checks overview invariants: visible_agents is a subset of agents, selected index in bounds.
    fn assert_overview_invariants(state: &AppState) {
        let visible = state.visible_agents();
        let all_ids: std::collections::HashSet<_> = state.agents.iter().map(|a| a.id()).collect();

        // Every visible agent must exist in the authoritative agent list
        for agent in &visible {
            assert!(
                all_ids.contains(agent.id()),
                "visible agent {:?} not found in agents list",
                agent.id()
            );
        }

        // Visible count must never exceed total agent count
        assert!(
            visible.len() <= state.agents.len(),
            "visible agents ({}) exceeds total agents ({})",
            visible.len(),
            state.agents.len()
        );

        // When no filter is active, all non-exited agents should be visible
        // (we don't re-implement filter matching — just verify the subset property)

        // Selected index must be within visible bounds
        if let Some(idx) = state.root.overview.selected_index {
            assert!(
                idx < visible.len(),
                "overview selected_index ({}) out of bounds for {} visible agents",
                idx,
                visible.len()
            );
        }
    }

    #[test]
    fn overview_invariants_hold_after_filter_changes() {
        let mut state = AppState::new();
        state.handle_event(discovered(AgentTool::Claude, 1, "/tmp/claude"));
        state.handle_event(discovered(AgentTool::OpenCode, 2, "/tmp/opencode"));
        assert_overview_invariants(&state);

        state.root.overview.filter = Some("claude".to_string());
        state.clamp_selection();
        assert_overview_invariants(&state);

        state.root.overview.filter = Some("nonexistent".to_string());
        state.clamp_selection();
        assert_overview_invariants(&state);

        state.root.overview.filter = None;
        state.clamp_selection();
        assert_overview_invariants(&state);
    }

    #[test]
    fn overview_invariants_hold_after_agent_exits() {
        let mut state = AppState::new();
        state.handle_event(discovered(AgentTool::Claude, 1, "/tmp/claude"));
        state.handle_event(discovered(AgentTool::OpenCode, 2, "/tmp/opencode"));
        assert_overview_invariants(&state);

        state.handle_event(AgentEvent::ProcessExited {
            id: AgentId::new(AgentTool::Claude, 1),
            pid: Some(1),
        });
        assert_overview_invariants(&state);
    }

    #[test]
    fn detector_health_does_not_affect_agent_list() {
        let mut state = AppState::new();

        state.handle_event(AgentEvent::DetectorHealthChanged {
            detector: DetectorKind::Claude,
            healthy: true,
            message: "ok".into(),
        });

        assert!(state.visible_agents().is_empty());
    }

    #[test]
    fn test_import_codex_auth_writes_profile_json() {
        let tmp = TempDir::new().unwrap();
        let profiles_dir = tmp.path().join("profiles");
        fs::create_dir_all(&profiles_dir).unwrap();

        let auth_source = tmp.path().join("auth.json");
        fs::write(&auth_source, b"{\"email\":\"test@example.com\"}").unwrap();

        let profile_id = "profile-01";
        let profile_dir = profiles_dir.join(profile_id);
        let profile = StoredCodexProfile::import_from_auth(
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
                    profile_id: "c1".into(),
                },
                AccountEntry {
                    provider: Provider::Codex,
                    profile_id: "c2".into(),
                },
                AccountEntry {
                    provider: Provider::Claude,
                    profile_id: "cl1".into(),
                },
            ],
            ..AccountsState::default()
        };

        assert_eq!(state.visible_entries().len(), 3);

        state.provider_filter = ProviderFilter::Codex;
        let codex_only = state.visible_entries();
        assert_eq!(codex_only.len(), 2);
        assert!(codex_only
            .iter()
            .all(|entry| entry.provider == Provider::Codex));

        state.provider_filter = ProviderFilter::Claude;
        let claude_only = state.visible_entries();
        assert_eq!(claude_only.len(), 1);
        assert_eq!(claude_only[0].provider, Provider::Claude);
    }

    #[test]
    fn active_tab_tracks_root_workspace_state() {
        let mut state = AppState::new();
        state.root.switch_workspace(Workspace::Accounts);
        assert_eq!(state.active_tab(), Workspace::Accounts);
    }

    #[test]
    fn app_state_workspace_navigation_uses_root_workspace_state() {
        let mut state = AppState::new();

        assert_eq!(state.root.workspace.active, Workspace::Overview);

        state.next_tab();
        assert_eq!(state.root.workspace.active, Workspace::Accounts);

        state.previous_tab();
        assert_eq!(state.root.workspace.active, Workspace::Overview);
    }

    #[test]
    fn app_state_help_overlay_uses_root_system_overlay() {
        let mut state = AppState::new();

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
