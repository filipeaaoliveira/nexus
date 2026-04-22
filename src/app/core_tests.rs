use super::*;
use crate::app::state::AccountsState;
use nexus_domain::accounts::ProfileId;
use crate::codex_profiles::profile::{self as codex_profile};
use crate::profiles::{io as profile_io, ProfileMetadata};
use crate::event::{AgentEvent, AgentRef};
use crate::services::accounts::storage::next_profile_id;
use nexus_domain::agents::ids::{AgentId, AgentTool};
use nexus_domain::agents::status::PrimaryStatus;
use nexus_domain::system::DetectorKind;
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
        agent: AgentRef::new(AgentId::new(tool, pid), Some(pid)),
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

    state.run_overview_reducer_action(OverviewAction::UpdateFilter("claude".to_string()));
    assert_eq!(state.visible_agents().len(), 1);
    assert_eq!(state.visible_agents()[0].pid(), Some(1));

    state.run_overview_reducer_action(OverviewAction::UpdateFilter("opencode".to_string()));
    assert_eq!(state.visible_agents().len(), 1);
    assert_eq!(state.visible_agents()[0].pid(), Some(2));
}

#[test]
fn visible_agent_indices_refresh_when_sort_changes() {
    let mut state = AppState::new();
    state.handle_event(discovered(AgentTool::OpenCode, 2, "/tmp/opencode"));
    state.handle_event(discovered(AgentTool::Claude, 1, "/tmp/claude"));

    assert_eq!(state.visible_agents()[0].pid(), Some(1));

    // Default is Name; one cycle: Name -> Elapsed
    state.run_overview_reducer_action(OverviewAction::CycleSortMode);
    assert_eq!(state.visible_agents()[0].pid(), Some(2));

    state.handle_event(AgentEvent::StatusChanged {
        agent: AgentRef::new(AgentId::new(AgentTool::OpenCode, 2), Some(2)),
        old: PrimaryStatus::Running,
        new: PrimaryStatus::Waiting,
    });
    // Elapsed -> Urgency
    state.run_overview_reducer_action(OverviewAction::CycleSortMode);
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
    state.run_overview_reducer_action(OverviewAction::UpdateFilter("review".to_string()));

    let visible = state.visible_agents();
    assert_eq!(visible.len(), 1);
    assert_eq!(*visible[0].id(), AgentId::new(AgentTool::Claude, 1));
}

/// Checks overview invariants: visible_agents is a subset of agents, selected index in bounds.
fn assert_overview_invariants(state: &AppState) {
    let visible = state.visible_agents();
    let all_ids: std::collections::HashSet<_> = state.agents().iter().map(|a| a.id()).collect();

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
        visible.len() <= state.agents().len(),
        "visible agents ({}) exceeds total agents ({})",
        visible.len(),
        state.agents.len()
    );

    // When no filter is active, all non-exited agents should be visible
    // (we don't re-implement filter matching — just verify the subset property)

    // Selected index must be within visible bounds
    if let Some(idx) = state.overview().selected_index() {
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

    state.run_overview_reducer_action(OverviewAction::UpdateFilter("claude".to_string()));
    assert_overview_invariants(&state);

    state.run_overview_reducer_action(OverviewAction::UpdateFilter("nonexistent".to_string()));
    assert_overview_invariants(&state);

    state.run_overview_reducer_action(OverviewAction::ClearFilter);
    assert_overview_invariants(&state);
}

#[test]
fn overview_invariants_hold_after_agent_exits() {
    let mut state = AppState::new();
    state.handle_event(discovered(AgentTool::Claude, 1, "/tmp/claude"));
    state.handle_event(discovered(AgentTool::OpenCode, 2, "/tmp/opencode"));
    assert_overview_invariants(&state);

    state.handle_event(AgentEvent::ProcessExited {
        agent: AgentRef::new(AgentId::new(AgentTool::Claude, 1), Some(1)),
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
    let profile =
        codex_profile::import_from_auth(profile_id, profile_id, &profile_dir, &auth_source)
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
    profile_io::save_profile_metadata(&profile_dir, &metadata).unwrap();

    let written = profile_io::load_profile_metadata(&profile_dir).unwrap();
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
                profile_id: ProfileId::new_unchecked("c1"),
            },
            AccountEntry {
                provider: Provider::Codex,
                profile_id: ProfileId::new_unchecked("c2"),
            },
            AccountEntry {
                provider: Provider::Claude,
                profile_id: ProfileId::new_unchecked("cl1"),
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

    assert_eq!(state.active_tab(), Workspace::Overview);

    state.next_tab();
    assert_eq!(state.active_tab(), Workspace::Accounts);

    state.previous_tab();
    assert_eq!(state.active_tab(), Workspace::Overview);
}

#[test]
fn app_state_help_overlay_uses_root_system_overlay() {
    let mut state = AppState::new();

    assert_eq!(*state.system().overlay(), Overlay::None);

    state.toggle_help();
    assert_eq!(*state.system().overlay(), Overlay::Help { scroll: 0 });

    state.scroll_help_down();
    match *state.system().overlay() {
        Overlay::Help { scroll } => assert!(scroll > 0),
        Overlay::None => panic!("help overlay should stay visible while scrolling"),
    }

    state.toggle_help();
    assert_eq!(*state.system().overlay(), Overlay::None);
}
