use nexus::app::{ActiveTab, AppState, AccountsTabState};
use nexus::config::Config;
use nexus::event::AgentEvent;
use nexus::types::*;
use nexus::ui::{footer, help};
use std::path::PathBuf;
use tempfile::tempdir;

fn make_state() -> AppState {
    let temp = tempdir().expect("tempdir");
    let root = temp.keep();

    let mut config = Config::default();
    config.codex_management_dir = root.join("codex-management");
    config.codex_profiles_dir = config.codex_management_dir.join("profiles");
    config.codex_repos_dir = config.codex_management_dir.join("repos");
    config.codex_runtime_dir = config.codex_management_dir.join("runtime");
    config.codex_quota_cache_file = config.codex_management_dir.join("quota-cache.json");
    config.codex_data_dir = root.join("codex-monitor");
    config.claude_data_dir = root.join("claude");
    config.opencode_data_dir = root.join("opencode");

    AppState::new(config)
}

fn discovered(
    tool: AgentTool,
    pid: u32,
    cwd: impl Into<PathBuf>,
    started_at: Option<chrono::DateTime<chrono::Local>>,
) -> AgentEvent {
    AgentEvent::Discovered {
        id: AgentId::new(tool, pid),
        pid: Some(pid),
        cwd: cwd.into(),
        started_at,
    }
}

fn session_info(
    tool: AgentTool,
    pid: u32,
    session_id: impl Into<String>,
    branch: Option<String>,
    is_worktree: bool,
    started_at: Option<chrono::DateTime<chrono::Local>>,
) -> AgentEvent {
    let session_id = session_id.into();
    let id = if tool == AgentTool::Codex && !session_id.is_empty() {
        AgentId::session(tool, session_id.clone())
    } else {
        AgentId::new(tool, pid)
    };
    AgentEvent::SessionInfo {
        id,
        pid: Some(pid),
        session_id,
        branch,
        is_worktree,
        started_at,
    }
}

fn status_changed(tool: AgentTool, pid: u32, old: PrimaryStatus, new: PrimaryStatus) -> AgentEvent {
    AgentEvent::StatusChanged {
        id: AgentId::new(tool, pid),
        pid: Some(pid),
        old,
        new,
    }
}

fn normalized_state_changed(tool: AgentTool, pid: u32, state: NormalizedState) -> AgentEvent {
    AgentEvent::NormalizedStateChanged {
        id: AgentId::new(tool, pid),
        pid: Some(pid),
        state,
    }
}

fn process_exited(tool: AgentTool, pid: u32) -> AgentEvent {
    AgentEvent::ProcessExited {
        id: AgentId::new(tool, pid),
        pid: Some(pid),
    }
}

fn subagent_spawned(tool: AgentTool, pid: u32, subagent: Subagent) -> AgentEvent {
    AgentEvent::SubagentSpawned {
        id: AgentId::new(tool, pid),
        pid: Some(pid),
        subagent,
    }
}

fn subagent_completed(tool: AgentTool, pid: u32, subagent_name: String) -> AgentEvent {
    AgentEvent::SubagentCompleted {
        id: AgentId::new(tool, pid),
        pid: Some(pid),
        subagent_name,
    }
}

fn todos_updated(tool: AgentTool, pid: u32, todos: Vec<TodoItem>) -> AgentEvent {
    AgentEvent::TodosUpdated {
        id: AgentId::new(tool, pid),
        pid: Some(pid),
        todos,
    }
}

#[test]
fn test_initial_state_is_empty() {
    let state = make_state();
    assert!(state.agents.is_empty());
    assert_eq!(state.selected, None);
    assert_eq!(state.sort_mode, SortMode::Name);
    assert!(!state.show_help);
}

#[test]
fn test_initial_state_starts_on_overview_tab() {
    let state = make_state();
    assert_eq!(state.active_tab(), ActiveTab::Overview);
}

#[test]
fn codex_tab_selection_wraps() {
    let mut state = AccountsTabState::new();
    state.codex_profile_ids = vec!["a".into(), "b".into()];
    state.selected_index = Some(1);
    let codex_ids = state.codex_profile_ids.clone();
    let claude_ids = state.claude_profile_ids.clone();
    state.next_profile(&codex_ids, &claude_ids);
    assert_eq!(state.selected_index, Some(0));
}

#[test]
fn codex_tab_next_profile_selects_first_when_unset() {
    let mut state = AccountsTabState::new();
    state.codex_profile_ids = vec!["a".into(), "b".into()];
    let codex_ids = state.codex_profile_ids.clone();
    let claude_ids = state.claude_profile_ids.clone();
    state.next_profile(&codex_ids, &claude_ids);
    assert_eq!(state.selected_index, Some(0));
}

#[test]
fn test_tab_navigation_cycles_between_tabs() {
    let mut state = make_state();
    assert_eq!(state.active_tab(), ActiveTab::Overview);

    state.next_tab();
    assert_eq!(state.active_tab(), ActiveTab::Accounts);

    state.previous_tab();
    assert_eq!(state.active_tab(), ActiveTab::Overview);

    state.previous_tab();
    assert_eq!(state.active_tab(), ActiveTab::Accounts);

    state.next_tab();
    assert_eq!(state.active_tab(), ActiveTab::Overview);
}

#[test]
fn codex_tab_navigation_keys_do_not_move_overview_selection() {
    let mut state = make_state();
    state.next_tab();
    assert_eq!(state.active_tab(), ActiveTab::Accounts);

    state.accounts_tab.codex_profile_ids = vec!["one".into(), "two".into()];
    state.handle_codex_move_down();

    assert_eq!(state.accounts_tab.selected_index, Some(0));
    assert_eq!(state.selected, None);
}

#[test]
fn codex_tab_filter_input_stays_on_codex_state() {
    let mut state = make_state();
    state.next_tab();
    assert_eq!(state.active_tab(), ActiveTab::Accounts);

    state.start_codex_filter();
    state.codex_filter_push_char('q');
    state.codex_filter_push_char('1');

    assert!(state.filter.is_none());
    assert!(!state.filter_mode);
    assert_eq!(state.filter_input, "");
    assert!(state.accounts_tab.filter_mode);
    assert_eq!(state.accounts_tab.filter_input, "q1");

    state.confirm_codex_filter();
    assert!(!state.accounts_tab.filter_mode);
    assert_eq!(state.accounts_tab.filter_input, "q1");
}

#[test]
fn codex_footer_uses_codex_shortcuts() {
    let mut state = make_state();
    state.next_tab();

    let footer = footer::shortcut_text(&state);
    assert!(footer.contains("import codex"));
    assert!(footer.contains("import claude"));
    assert!(!footer.contains("sort:"));
    assert!(!footer.contains(" rename"));
}

#[test]
fn help_overlay_lists_codex_import_and_launch_shortcuts() {
    let lines = help::help_lines(ActiveTab::Accounts, false);
    let content: Vec<String> = lines
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect()
        })
        .collect();

    assert!(content
        .iter()
        .any(|line| line.contains("Import ~/.codex/auth.json")));
    assert!(content
        .iter()
        .any(|line| line.contains("Import Claude credentials")));
    assert!(content.iter().any(|line| line.contains("Accounts")));
}

#[test]
fn test_discover_agent() {
    let mut state = make_state();
    state.handle_event(discovered(
        AgentTool::Claude,
        1234,
        PathBuf::from("/tmp/project"),
        None,
    ));
    assert_eq!(state.agents.len(), 1);
    assert_eq!(state.agents[0].pid, Some(1234));
    assert_eq!(state.agents[0].id.tool(), AgentTool::Claude);
    assert_eq!(state.agents[0].status, PrimaryStatus::Running);
    assert_eq!(state.selected, Some(0));
}

#[test]
fn test_discover_duplicate_ignored() {
    let mut state = make_state();
    state.handle_event(discovered(
        AgentTool::Claude,
        1234,
        PathBuf::from("/tmp/project"),
        None,
    ));
    state.handle_event(discovered(
        AgentTool::Claude,
        1234,
        PathBuf::from("/tmp/project"),
        None,
    ));
    assert_eq!(state.agents.len(), 1);
}

#[test]
fn test_status_change() {
    let mut state = make_state();
    state.handle_event(discovered(
        AgentTool::OpenCode,
        5678,
        PathBuf::from("/tmp/oc"),
        None,
    ));
    state.handle_event(status_changed(
        AgentTool::OpenCode,
        5678,
        PrimaryStatus::Running,
        PrimaryStatus::Waiting,
    ));
    assert_eq!(state.agents[0].status, PrimaryStatus::Waiting);
}

#[test]
fn test_process_exited() {
    let mut state = make_state();
    state.handle_event(discovered(
        AgentTool::Claude,
        9999,
        PathBuf::from("/tmp/cx"),
        None,
    ));
    state.handle_event(process_exited(AgentTool::Claude, 9999));
    assert!(state.agents.is_empty());
}

#[test]
fn test_subagent_spawned_and_completed() {
    let mut state = make_state();
    state.handle_event(discovered(
        AgentTool::OpenCode,
        1000,
        PathBuf::from("/tmp"),
        None,
    ));
    state.handle_event(subagent_spawned(
        AgentTool::OpenCode,
        1000,
        Subagent {
            name: "explore".to_string(),
            status: PrimaryStatus::Running,
            description: Some("searching codebase".to_string()),
            started_at: chrono::Local::now(),
            ended_at: None,
        },
    ));
    assert_eq!(state.agents[0].subagents.len(), 1);
    assert_eq!(state.agents[0].subagents[0].name, "explore");
    state.handle_event(subagent_completed(
        AgentTool::OpenCode,
        1000,
        "explore".to_string(),
    ));
    assert_eq!(state.agents[0].subagents[0].status, PrimaryStatus::Done);
    assert!(state.agents[0].subagents[0].ended_at.is_some());
}

#[test]
fn test_move_selection() {
    let mut state = make_state();
    for pid in [100, 200, 300] {
        state.handle_event(discovered(
            AgentTool::Claude,
            pid,
            PathBuf::from(format!("/tmp/{pid}")),
            None,
        ));
    }
    assert_eq!(state.selected, Some(0));
    state.move_selection_down();
    assert_eq!(state.selected, Some(1));
    state.move_selection_down();
    assert_eq!(state.selected, Some(2));
    state.move_selection_down();
    assert_eq!(state.selected, Some(0)); // wraps to top
    state.move_selection_up();
    assert_eq!(state.selected, Some(2)); // wraps to bottom
    state.move_selection_up();
    assert_eq!(state.selected, Some(1));
    state.move_selection_up();
    assert_eq!(state.selected, Some(0));
}

#[test]
fn test_count_by_status() {
    let mut state = make_state();
    state.handle_event(discovered(AgentTool::Claude, 1, PathBuf::from("/a"), None));
    state.handle_event(discovered(
        AgentTool::OpenCode,
        2,
        PathBuf::from("/b"),
        None,
    ));
    state.handle_event(status_changed(
        AgentTool::Claude,
        1,
        PrimaryStatus::Running,
        PrimaryStatus::Waiting,
    ));
    state.handle_event(AgentEvent::SessionInfo {
        id: AgentId::session(AgentTool::Codex, "codex-hidden"),
        pid: None,
        session_id: "codex-hidden".to_string(),
        branch: Some("main".to_string()),
        is_worktree: false,
        started_at: None,
    });
    assert_eq!(state.count_by_status(PrimaryStatus::Waiting), 1);
    assert_eq!(state.count_by_status(PrimaryStatus::Running), 1);
}

#[test]
fn test_cycle_sort_mode() {
    let mut state = make_state();
    assert_eq!(state.sort_mode, SortMode::Name);
    state.cycle_sort_mode();
    assert_eq!(state.sort_mode, SortMode::Elapsed);
    state.cycle_sort_mode();
    assert_eq!(state.sort_mode, SortMode::Urgency);
    state.cycle_sort_mode();
    assert_eq!(state.sort_mode, SortMode::Name);
}

#[test]
fn test_toggle_help() {
    let mut state = make_state();
    assert!(!state.show_help);
    state.toggle_help();
    assert!(state.show_help);
    state.toggle_help();
    assert!(!state.show_help);
}

#[test]
fn test_visible_agents_sort_by_urgency() {
    let mut state = make_state();
    state.sort_mode = SortMode::Urgency;
    // Add agents with different statuses
    for (pid, tool) in [
        (1, AgentTool::Claude),
        (2, AgentTool::OpenCode),
        (3, AgentTool::Codex),
    ] {
        state.handle_event(discovered(
            tool,
            pid,
            PathBuf::from(format!("/tmp/{pid}")),
            None,
        ));
    }
    // Make agent 3 Waiting, agent 1 Done
    state.handle_event(status_changed(
        AgentTool::Codex,
        3,
        PrimaryStatus::Running,
        PrimaryStatus::Waiting,
    ));
    state.handle_event(status_changed(
        AgentTool::Claude,
        1,
        PrimaryStatus::Running,
        PrimaryStatus::Done,
    ));
    // Urgency sort: Waiting < Running < Done
    let visible = state.visible_agents();
    assert_eq!(visible[0].pid.unwrap(), 3); // Waiting
    assert_eq!(visible[1].pid.unwrap(), 2); // Running
    assert_eq!(visible[2].pid.unwrap(), 1); // Done
}

#[test]
fn test_visible_agents_sort_by_urgency_uses_normalized_precedence() {
    let mut state = make_state();
    state.sort_mode = SortMode::Urgency;

    for (pid, normalized) in [
        (
            1,
            NormalizedState::new(
                NormalizedStatus::Done,
                Some(NormalizedReason::ReadyForNextTask),
            ),
        ),
        (
            2,
            NormalizedState::new(NormalizedStatus::Working, Some(NormalizedReason::ToolCall)),
        ),
        (
            3,
            NormalizedState::new(NormalizedStatus::NeedsInput, Some(NormalizedReason::Review)),
        ),
    ] {
        state.handle_event(discovered(
            AgentTool::Claude,
            pid,
            PathBuf::from(format!("/tmp/{pid}")),
            None,
        ));
        state.handle_event(normalized_state_changed(AgentTool::Claude, pid, normalized));
    }

    let visible = state.visible_agents();
    assert_eq!(visible[0].pid.unwrap(), 3);
    assert_eq!(visible[1].pid.unwrap(), 2);
    assert_eq!(visible[2].pid.unwrap(), 1);
}

#[test]
fn test_visible_agents_sort_by_name() {
    let mut state = make_state();
    state.sort_mode = SortMode::Name;
    for (pid, tool) in [
        (1, AgentTool::OpenCode),
        (2, AgentTool::Claude),
        (3, AgentTool::Codex),
    ] {
        state.handle_event(discovered(
            tool,
            pid,
            PathBuf::from(format!("/tmp/{pid}")),
            None,
        ));
    }
    let visible = state.visible_agents();
    // Alphabetical: claude < codex < opencode
    assert_eq!(visible[0].id.tool(), AgentTool::Claude);
    assert_eq!(visible[1].id.tool(), AgentTool::Codex);
    assert_eq!(visible[2].id.tool(), AgentTool::OpenCode);
}

#[test]
fn test_visible_agents_filter() {
    let mut state = make_state();
    for (pid, tool) in [
        (1, AgentTool::Claude),
        (2, AgentTool::OpenCode),
        (3, AgentTool::Codex),
    ] {
        state.handle_event(discovered(
            tool,
            pid,
            PathBuf::from(format!("/tmp/{pid}")),
            None,
        ));
    }
    // Filter for "claude"
    state.filter = Some("claude".to_string());
    let visible = state.visible_agents();
    assert_eq!(visible.len(), 1);
    assert_eq!(visible[0].id.tool(), AgentTool::Claude);

    // Filter for status "RUNNING" — should match all
    state.filter = Some("running".to_string());
    let visible = state.visible_agents();
    assert_eq!(visible.len(), 3);

    // Filter for something that matches nothing
    state.filter = Some("zzz".to_string());
    let visible = state.visible_agents();
    assert_eq!(visible.len(), 0);
}

#[test]
fn test_filter_by_tool_name() {
    let mut state = make_state();
    state.handle_event(discovered(AgentTool::Claude, 1, PathBuf::from("/a"), None));
    state.handle_event(discovered(
        AgentTool::OpenCode,
        2,
        PathBuf::from("/b"),
        None,
    ));
    state.handle_event(discovered(AgentTool::Codex, 3, PathBuf::from("/c"), None));

    state.filter = Some("claude".to_string());
    let visible = state.visible_agents();
    assert_eq!(visible.len(), 1);
    assert_eq!(visible[0].id.tool(), AgentTool::Claude);
}

#[test]
fn test_filter_by_branch() {
    let mut state = make_state();
    state.handle_event(discovered(AgentTool::Claude, 1, PathBuf::from("/a"), None));
    state.handle_event(session_info(
        AgentTool::Claude,
        1,
        "s1".to_string(),
        Some("feat/auth".to_string()),
        false,
        None,
    ));
    state.handle_event(discovered(
        AgentTool::OpenCode,
        2,
        PathBuf::from("/b"),
        None,
    ));
    state.handle_event(session_info(
        AgentTool::OpenCode,
        2,
        "s2".to_string(),
        Some("main".to_string()),
        false,
        None,
    ));

    state.filter = Some("auth".to_string());
    let visible = state.visible_agents();
    assert_eq!(visible.len(), 1);
    assert_eq!(visible[0].pid.unwrap(), 1);
}

#[test]
fn test_filter_by_status() {
    let mut state = make_state();
    state.handle_event(discovered(AgentTool::Claude, 1, PathBuf::from("/a"), None));
    state.handle_event(status_changed(
        AgentTool::Claude,
        1,
        PrimaryStatus::Running,
        PrimaryStatus::Waiting,
    ));
    state.handle_event(discovered(
        AgentTool::OpenCode,
        2,
        PathBuf::from("/b"),
        None,
    ));

    state.filter = Some("waiting".to_string());
    let visible = state.visible_agents();
    assert_eq!(visible.len(), 1);
    assert_eq!(visible[0].pid.unwrap(), 1);
}

#[test]
fn test_filter_matches_normalized_status_and_reason() {
    let mut state = make_state();
    state.handle_event(discovered(AgentTool::Claude, 1, PathBuf::from("/a"), None));
    state.handle_event(normalized_state_changed(
        AgentTool::Claude,
        1,
        NormalizedState::new(NormalizedStatus::NeedsInput, Some(NormalizedReason::Review)),
    ));

    state.filter = Some("needs input".to_string());
    let visible = state.visible_agents();
    assert_eq!(visible.len(), 1);
    assert_eq!(visible[0].pid.unwrap(), 1);

    state.filter = Some("review".to_string());
    let visible = state.visible_agents();
    assert_eq!(visible.len(), 1);
    assert_eq!(visible[0].pid.unwrap(), 1);
}

#[test]
fn test_filter_clamps_selected_index() {
    let mut state = make_state();
    // Add 4 agents
    for pid in [1, 2, 3, 4] {
        state.handle_event(discovered(
            AgentTool::Claude,
            pid,
            PathBuf::from(format!("/tmp/{pid}")),
            None,
        ));
    }
    // Select the last agent (index 3)
    state.selected = Some(3);

    // Apply a filter that matches only 1 agent by cwd
    state.start_filter();
    state.filter_push_char('1');

    // Only 1 visible agent, so selected must be clamped to 0
    assert_eq!(state.visible_agents().len(), 1);
    assert_eq!(state.selected, Some(0));
}

#[test]
fn test_discover_with_real_start_time() {
    let mut state = make_state();
    let past = chrono::Local::now() - chrono::Duration::hours(2);
    state.handle_event(discovered(
        AgentTool::OpenCode,
        42,
        PathBuf::from("/tmp/test"),
        Some(past),
    ));
    // The agent's started_at should be the past time, not now
    assert_eq!(state.agents[0].started_at, past);
}

#[test]
fn test_discover_without_start_time_uses_now() {
    let before = chrono::Local::now();
    let mut state = make_state();
    state.handle_event(discovered(
        AgentTool::Claude,
        43,
        PathBuf::from("/tmp/test2"),
        None,
    ));
    let after = chrono::Local::now();
    assert!(state.agents[0].started_at >= before);
    assert!(state.agents[0].started_at <= after);
}

#[test]
fn test_session_info_before_discovered_is_preserved() {
    let mut state = make_state();

    state.handle_event(AgentEvent::SessionInfo {
        id: AgentId::new(AgentTool::Claude, 42),
        pid: Some(42),
        session_id: "session-a".to_string(),
        branch: Some("feature/early".to_string()),
        is_worktree: true,
        started_at: None,
    });

    assert_eq!(state.agents.len(), 1);
    assert_eq!(state.agents[0].session_id, Some("session-a".to_string()));
    assert_eq!(state.agents[0].branch, Some("feature/early".to_string()));

    state.handle_event(discovered(
        AgentTool::Claude,
        42,
        PathBuf::from("/tmp/project"),
        None,
    ));

    assert_eq!(state.agents.len(), 1);
    assert_eq!(state.agents[0].cwd, PathBuf::from("/tmp/project"));
    assert_eq!(state.agents[0].session_id, Some("session-a".to_string()));
    assert_eq!(state.agents[0].branch, Some("feature/early".to_string()));
}

#[test]
fn test_normalized_state_changed_before_discovered_is_preserved() {
    let mut state = make_state();

    state.handle_event(normalized_state_changed(
        AgentTool::Claude,
        77,
        NormalizedState::new(NormalizedStatus::Blocked, Some(NormalizedReason::Approval)),
    ));

    assert_eq!(state.agents.len(), 1);
    assert_eq!(state.agents[0].normalized.status, NormalizedStatus::Blocked);
    assert_eq!(
        state.agents[0].normalized.reason,
        Some(NormalizedReason::Approval)
    );

    state.handle_event(discovered(
        AgentTool::Claude,
        77,
        PathBuf::from("/tmp/blocked"),
        None,
    ));

    assert_eq!(state.agents.len(), 1);
    assert_eq!(state.agents[0].cwd, PathBuf::from("/tmp/blocked"));
    assert_eq!(state.agents[0].normalized.status, NormalizedStatus::Blocked);
    assert_eq!(
        state.agents[0].normalized.reason,
        Some(NormalizedReason::Approval)
    );
}

#[test]
fn test_session_info_before_discovered_selects_first_visible_agent() {
    let mut state = make_state();
    assert_eq!(state.selected, None);

    state.handle_event(AgentEvent::SessionInfo {
        id: AgentId::new(AgentTool::Claude, 42),
        pid: Some(42),
        session_id: "session-a".to_string(),
        branch: Some("feature/early".to_string()),
        is_worktree: false,
        started_at: None,
    });

    assert_eq!(state.visible_agents().len(), 1);
    assert_eq!(state.selected, Some(0));
}

#[test]
fn test_remove_exited_agents() {
    let mut state = make_state();
    state.handle_event(discovered(
        AgentTool::Claude,
        100,
        PathBuf::from("/tmp/a"),
        None,
    ));
    state.handle_event(discovered(
        AgentTool::OpenCode,
        200,
        PathBuf::from("/tmp/b"),
        None,
    ));
    state.handle_event(discovered(
        AgentTool::Codex,
        300,
        PathBuf::from("/tmp/c"),
        None,
    ));
    // Exit agent 100 and 300
    state.handle_event(process_exited(AgentTool::Claude, 100));
    state.handle_event(process_exited(AgentTool::Codex, 300));
    state.selected = Some(0);
    state.remove_exited();
    assert_eq!(state.agents.len(), 1);
    assert_eq!(state.agents[0].pid.unwrap(), 200);
    assert_eq!(state.selected, Some(0));
}

#[test]
fn test_remove_exited_no_exited_agents() {
    let mut state = make_state();
    state.handle_event(discovered(
        AgentTool::Claude,
        100,
        PathBuf::from("/tmp/a"),
        None,
    ));
    state.selected = Some(0);
    state.remove_exited();
    assert_eq!(state.agents.len(), 1); // no change
    assert_eq!(state.selected, Some(0));
}

#[test]
fn test_remove_exited_all_exited() {
    let mut state = make_state();
    state.handle_event(discovered(
        AgentTool::Claude,
        100,
        PathBuf::from("/tmp/a"),
        None,
    ));
    state.handle_event(process_exited(AgentTool::Claude, 100));
    state.selected = Some(0);
    state.remove_exited();
    assert_eq!(state.agents.len(), 0);
    assert_eq!(state.selected, None);
}

#[test]
fn test_remove_selected_agent() {
    let mut state = make_state();
    state.handle_event(discovered(
        AgentTool::Claude,
        100,
        PathBuf::from("/tmp/a"),
        None,
    ));
    state.handle_event(discovered(
        AgentTool::OpenCode,
        200,
        PathBuf::from("/tmp/b"),
        None,
    ));
    assert_eq!(state.agents.len(), 2);
    state.selected = Some(0);
    state.remove_selected();
    assert_eq!(state.agents.len(), 1);
    assert_eq!(state.agents[0].pid.unwrap(), 200);
    assert_eq!(state.selected, Some(0));
}

#[test]
fn test_remove_selected_last_item_clamps() {
    let mut state = make_state();
    state.handle_event(discovered(
        AgentTool::Claude,
        100,
        PathBuf::from("/tmp/a"),
        None,
    ));
    state.handle_event(discovered(
        AgentTool::OpenCode,
        200,
        PathBuf::from("/tmp/b"),
        None,
    ));
    state.selected = Some(1);
    state.remove_selected();
    assert_eq!(state.agents.len(), 1);
    assert_eq!(state.selected, Some(0));
}

#[test]
fn test_remove_selected_only_item() {
    let mut state = make_state();
    state.handle_event(discovered(
        AgentTool::Claude,
        100,
        PathBuf::from("/tmp/a"),
        None,
    ));
    state.selected = Some(0);
    state.remove_selected();
    assert_eq!(state.agents.len(), 0);
    assert_eq!(state.selected, None);
}

#[test]
fn test_remove_selected_none_selected() {
    let mut state = make_state();
    state.handle_event(discovered(
        AgentTool::Claude,
        100,
        PathBuf::from("/tmp/a"),
        None,
    ));
    state.selected = None;
    state.remove_selected();
    assert_eq!(state.agents.len(), 1); // no change
}

#[test]
fn test_remove_selected_with_filter_removes_correct_agent() {
    let mut state = make_state();
    // Add claude (pid 100) and opencode (pid 200)
    state.handle_event(discovered(
        AgentTool::Claude,
        100,
        PathBuf::from("/tmp/a"),
        None,
    ));
    state.handle_event(discovered(
        AgentTool::OpenCode,
        200,
        PathBuf::from("/tmp/b"),
        None,
    ));
    // Filter to show only opencode
    state.filter = Some("opencode".to_string());
    state.selected = Some(0); // first visible = opencode
    state.remove_selected();
    // opencode should be removed, claude should remain
    assert_eq!(state.agents.len(), 1);
    assert_eq!(state.agents[0].id.tool(), AgentTool::Claude);
}

#[test]
fn test_rename_with_filter_renames_correct_agent() {
    let mut state = make_state();
    // Add claude (pid 100) and opencode (pid 200)
    state.handle_event(discovered(
        AgentTool::Claude,
        100,
        PathBuf::from("/tmp/a"),
        None,
    ));
    state.handle_event(discovered(
        AgentTool::OpenCode,
        200,
        PathBuf::from("/tmp/b"),
        None,
    ));
    // Filter to show only opencode
    state.filter = Some("opencode".to_string());
    state.selected = Some(0); // first visible = opencode
    state.start_rename();
    state.rename_input = "my-oc".to_string();
    state.confirm_rename();
    // opencode should be renamed, claude untouched
    let oc = state.agents.iter().find(|a| a.pid == Some(200)).unwrap();
    assert_eq!(oc.alias, Some("my-oc".to_string()));
    let cl = state.agents.iter().find(|a| a.pid == Some(100)).unwrap();
    assert_eq!(cl.alias, None);
}

#[test]
fn test_session_info_only_applies_earlier_start_time() {
    let mut state = make_state();
    let now = chrono::Local::now();
    state.handle_event(discovered(
        AgentTool::OpenCode,
        44,
        PathBuf::from("/tmp/test3"),
        None,
    ));

    // SessionInfo with earlier time should update
    let earlier = now - chrono::Duration::hours(1);
    state.handle_event(session_info(
        AgentTool::OpenCode,
        44,
        "s1".to_string(),
        None,
        false,
        Some(earlier),
    ));
    assert_eq!(state.agents[0].started_at, earlier);

    // SessionInfo with later time should NOT update
    let later = now + chrono::Duration::hours(1);
    state.handle_event(session_info(
        AgentTool::OpenCode,
        44,
        "s1".to_string(),
        None,
        false,
        Some(later),
    ));
    // Should still be the earlier time
    assert_eq!(state.agents[0].started_at, earlier);
}

#[test]
fn test_session_info_none_started_at_does_not_overwrite_existing() {
    let mut state = make_state();
    let past = chrono::Local::now() - chrono::Duration::hours(1);

    // Discover agent with a known process start time
    state.handle_event(discovered(
        AgentTool::OpenCode,
        55,
        PathBuf::from("/tmp/oc-test"),
        Some(past),
    ));
    assert_eq!(state.agents[0].started_at, past);

    // SessionInfo with started_at: None must not overwrite the process start time
    state.handle_event(AgentEvent::SessionInfo {
        id: AgentId::new(AgentTool::OpenCode, 55),
        pid: Some(55),
        session_id: "sess-55".to_string(),
        branch: None,
        is_worktree: false,
        started_at: None,
    });

    assert_eq!(
        state.agents[0].started_at, past,
        "SessionInfo with started_at: None must not overwrite the process start time"
    );
}

#[test]
fn test_codex_session_info_uses_session_identity() {
    let mut state = make_state();
    state.handle_event(discovered(
        AgentTool::Codex,
        44,
        PathBuf::from("/tmp/test3"),
        None,
    ));

    state.handle_event(session_info(
        AgentTool::Codex,
        44,
        "session-a".to_string(),
        None,
        false,
        None,
    ));
    state.handle_event(session_info(
        AgentTool::Codex,
        44,
        "session-b".to_string(),
        None,
        false,
        None,
    ));

    assert_eq!(state.agents.len(), 2);
    assert_ne!(state.agents[0].session_id, state.agents[1].session_id);
}

#[test]
fn test_codex_discovery_merges_into_existing_session_row() {
    let mut state = make_state();

    state.handle_event(session_info(
        AgentTool::Codex,
        44,
        "session-a".to_string(),
        None,
        false,
        None,
    ));
    state.handle_event(discovered(
        AgentTool::Codex,
        44,
        PathBuf::from("/tmp/merged"),
        None,
    ));

    assert_eq!(state.agents.len(), 1);
    assert_eq!(state.agents[0].cwd, PathBuf::from("/tmp/merged"));
    assert_eq!(state.agents[0].session_id, Some("session-a".to_string()));
    assert_eq!(state.agents[0].pid, Some(44));
}

#[test]
fn test_codex_discovery_does_not_guess_among_ambiguous_session_matches() {
    let mut state = make_state();

    state.handle_event(session_info(
        AgentTool::Codex,
        44,
        "session-a".to_string(),
        None,
        false,
        None,
    ));
    state.handle_event(session_info(
        AgentTool::Codex,
        44,
        "session-b".to_string(),
        None,
        false,
        None,
    ));
    state.handle_event(discovered(
        AgentTool::Codex,
        44,
        PathBuf::from("/tmp/ambiguous"),
        None,
    ));

    assert_eq!(state.agents.len(), 2);
    let session_agents: Vec<_> = state.agents.iter().filter(|a| a.id.is_session()).collect();
    assert_eq!(session_agents.len(), 2);
    assert!(session_agents
        .iter()
        .all(|a| a.cwd == std::path::Path::new("/tmp/ambiguous")));
    assert!(state
        .agents
        .iter()
        .all(|a| a.id.is_session() && a.pid == Some(44)));
}

#[test]
fn test_codex_process_exit_retires_all_session_rows_sharing_pid() {
    let mut state = make_state();

    state.handle_event(session_info(
        AgentTool::Codex,
        44,
        "session-a".to_string(),
        None,
        false,
        None,
    ));
    state.handle_event(session_info(
        AgentTool::Codex,
        44,
        "session-b".to_string(),
        None,
        false,
        None,
    ));

    state.handle_event(process_exited(AgentTool::Codex, 44));

    assert_eq!(state.agents.len(), 2);
    assert!(state
        .agents
        .iter()
        .all(|a| a.status == PrimaryStatus::Exited && a.pid == Some(44)));
}

#[test]
fn test_codex_process_only_row_is_removed_on_exit() {
    let mut state = make_state();

    state.handle_event(discovered(
        AgentTool::Codex,
        45337,
        PathBuf::from("/tmp/nexus"),
        None,
    ));
    assert_eq!(state.agents.len(), 1);
    assert!(state.agents[0].id.is_process());

    state.handle_event(process_exited(AgentTool::Codex, 45337));

    assert!(state.agents.is_empty());
    assert!(state.visible_agents().is_empty());
}

#[test]
fn test_codex_session_info_absorbs_stale_process_row_after_ambiguous_pid_resolves() {
    let mut state = make_state();

    state.handle_event(AgentEvent::Discovered {
        id: AgentId::session(AgentTool::Codex, "session-a"),
        pid: None,
        cwd: PathBuf::from("/tmp/ambiguous"),
        started_at: None,
    });
    state.handle_event(discovered(
        AgentTool::Codex,
        77,
        PathBuf::from("/tmp/ambiguous"),
        None,
    ));

    assert_eq!(state.agents.len(), 2);
    assert_eq!(state.agents.iter().filter(|a| a.id.is_session()).count(), 1);
    assert_eq!(state.agents.iter().filter(|a| a.id.is_process()).count(), 1);

    state.handle_event(session_info(
        AgentTool::Codex,
        77,
        "session-a".to_string(),
        None,
        false,
        None,
    ));

    assert_eq!(state.agents.len(), 1);
    assert!(state.agents[0].id.is_session());
    assert_eq!(state.agents[0].pid, Some(77));
    assert_eq!(state.agents[0].cwd, PathBuf::from("/tmp/ambiguous"));
    assert_eq!(state.agents[0].session_id, Some("session-a".to_string()));
}

#[test]
fn test_codex_session_info_preserves_terminal_status() {
    let mut state = make_state();

    state.handle_event(session_info(
        AgentTool::Codex,
        44,
        "session-a".to_string(),
        None,
        false,
        None,
    ));
    state.handle_event(AgentEvent::StatusChanged {
        id: AgentId::session(AgentTool::Codex, "session-a"),
        pid: Some(44),
        old: PrimaryStatus::Running,
        new: PrimaryStatus::Done,
    });

    assert_eq!(state.agents[0].status, PrimaryStatus::Done);

    state.handle_event(session_info(
        AgentTool::Codex,
        55,
        "session-a".to_string(),
        None,
        false,
        None,
    ));

    assert_eq!(state.agents.len(), 1);
    assert!(state.agents[0].id.is_session());
    assert_eq!(state.agents[0].status, PrimaryStatus::Done);
    assert_eq!(state.agents[0].pid, Some(55));
}

#[test]
fn test_codex_process_discovery_does_not_revive_done_session() {
    let mut state = make_state();

    state.handle_event(session_info(
        AgentTool::Codex,
        15642,
        "session-a".to_string(),
        None,
        false,
        None,
    ));
    state.handle_event(AgentEvent::StatusChanged {
        id: AgentId::session(AgentTool::Codex, "session-a"),
        pid: Some(15642),
        old: PrimaryStatus::Running,
        new: PrimaryStatus::Done,
    });

    state.handle_event(discovered(
        AgentTool::Codex,
        15642,
        PathBuf::from("/tmp/project"),
        None,
    ));

    assert_eq!(state.agents.len(), 1);
    assert!(state.agents[0].id.is_session());
    assert_eq!(state.agents[0].status, PrimaryStatus::Done);
    assert_eq!(state.agents[0].pid, Some(15642));
}

#[test]
fn test_codex_session_info_with_none_pid_preserves_existing_pid() {
    // A SessionInfo with pid=None must NOT clear a pid that was already bound.
    // The pid should only be cleared by ProcessExited, not by a stale snapshot
    // that omits the pid because the thread is outside the conservative binding
    // window.
    let mut state = make_state();

    state.handle_event(session_info(
        AgentTool::Codex,
        44,
        "session-a".to_string(),
        None,
        false,
        None,
    ));
    assert_eq!(state.agents[0].pid, Some(44));

    state.handle_event(AgentEvent::SessionInfo {
        id: AgentId::session(AgentTool::Codex, "session-a"),
        pid: None,
        session_id: "session-a".to_string(),
        branch: None,
        is_worktree: false,
        started_at: None,
    });

    assert_eq!(state.agents.len(), 1);
    assert!(state.agents[0].id.is_session());
    // pid must be preserved — not cleared — when SessionInfo arrives with pid=None
    assert_eq!(state.agents[0].pid, Some(44));
}

#[test]
fn test_codex_session_stays_visible_when_session_info_pid_is_none() {
    // When a Codex session already has a pid, a subsequent SessionInfo with
    // pid=None should preserve the pid so the session remains visible.
    let mut state = make_state();
    state.handle_event(discovered(
        AgentTool::Claude,
        10,
        PathBuf::from("/tmp/claude"),
        None,
    ));
    state.handle_event(AgentEvent::SessionInfo {
        id: AgentId::session(AgentTool::Codex, "codex-a"),
        pid: Some(20),
        session_id: "codex-a".to_string(),
        branch: Some("main".to_string()),
        is_worktree: false,
        started_at: None,
    });
    state.selected = Some(1);

    state.handle_event(AgentEvent::SessionInfo {
        id: AgentId::session(AgentTool::Codex, "codex-a"),
        pid: None,
        session_id: "codex-a".to_string(),
        branch: Some("main".to_string()),
        is_worktree: false,
        started_at: None,
    });

    // Both agents should still be visible (pid preserved on Codex session)
    assert_eq!(state.visible_agents().len(), 2);
    assert_eq!(state.selected, Some(1));
}

#[test]
fn test_codex_session_without_pid_is_not_visible() {
    let mut state = make_state();
    state.handle_event(AgentEvent::SessionInfo {
        id: AgentId::session(AgentTool::Codex, "codex-a"),
        pid: None,
        session_id: "codex-a".to_string(),
        branch: Some("main".to_string()),
        is_worktree: false,
        started_at: None,
    });
    assert!(state.visible_agents().is_empty());
}

#[test]
fn test_codex_session_with_pid_is_visible() {
    let mut state = make_state();
    state.handle_event(AgentEvent::SessionInfo {
        id: AgentId::session(AgentTool::Codex, "codex-a"),
        pid: Some(42),
        session_id: "codex-a".to_string(),
        branch: Some("main".to_string()),
        is_worktree: false,
        started_at: None,
    });
    assert_eq!(state.visible_agents().len(), 1);
}

#[test]
fn test_normalized_state_changed_updates_agent_normalized_state() {
    let mut state = make_state();
    let id = AgentId::new(AgentTool::Claude, 42);

    state.handle_event(discovered(
        AgentTool::Claude,
        42,
        PathBuf::from("/tmp/normalized"),
        None,
    ));
    state.handle_event(AgentEvent::NormalizedStateChanged {
        id,
        pid: Some(42),
        state: NormalizedState::new(NormalizedStatus::NeedsInput, Some(NormalizedReason::Review)),
    });

    assert_eq!(
        state.agents[0].normalized.status,
        NormalizedStatus::NeedsInput
    );
    assert_eq!(
        state.agents[0].normalized.reason,
        Some(NormalizedReason::Review)
    );
}

#[test]
fn test_codex_session_discovery_revives_exited_session_row() {
    let mut state = make_state();

    state.handle_event(session_info(
        AgentTool::Codex,
        44,
        "session-a".to_string(),
        None,
        false,
        None,
    ));
    state.handle_event(process_exited(AgentTool::Codex, 44));

    assert_eq!(state.agents[0].status, PrimaryStatus::Exited);

    state.handle_event(AgentEvent::Discovered {
        id: AgentId::session(AgentTool::Codex, "session-a"),
        pid: Some(44),
        cwd: PathBuf::from("/tmp/revive"),
        started_at: None,
    });

    assert_eq!(state.agents.len(), 1);
    assert!(state.agents[0].id.is_session());
    assert_eq!(state.agents[0].status, PrimaryStatus::Running);
    assert_eq!(state.agents[0].pid, Some(44));
    assert_eq!(state.agents[0].cwd, PathBuf::from("/tmp/revive"));
}

#[test]
fn test_codex_discovery_does_not_revive_exited_session_row_on_pid_reuse() {
    let mut state = make_state();

    state.handle_event(session_info(
        AgentTool::Codex,
        44,
        "session-a".to_string(),
        None,
        false,
        None,
    ));
    state.handle_event(process_exited(AgentTool::Codex, 44));

    state.handle_event(discovered(
        AgentTool::Codex,
        44,
        PathBuf::from("/tmp/reused"),
        None,
    ));

    assert_eq!(state.agents.len(), 2);
    assert_eq!(state.agents.iter().filter(|a| a.id.is_session()).count(), 1);
    assert_eq!(state.agents.iter().filter(|a| a.id.is_process()).count(), 1);

    let exited_session = state.agents.iter().find(|a| a.id.is_session()).unwrap();
    assert_eq!(exited_session.status, PrimaryStatus::Exited);
    assert_eq!(exited_session.pid, Some(44));

    let process_agent = state.agents.iter().find(|a| a.id.is_process()).unwrap();
    assert_eq!(process_agent.status, PrimaryStatus::Running);
    assert_eq!(process_agent.cwd, PathBuf::from("/tmp/reused"));
}

#[test]
fn test_codex_subagent_completion_matches_stable_key_and_keeps_human_name() {
    let mut state = make_state();
    let id = AgentId::session(AgentTool::Codex, "session-a");

    state.handle_event(AgentEvent::SessionInfo {
        id: id.clone(),
        pid: Some(44),
        session_id: "session-a".to_string(),
        branch: None,
        is_worktree: false,
        started_at: None,
    });
    state.handle_event(AgentEvent::SubagentSpawned {
        id: id.clone(),
        pid: Some(44),
        subagent: Subagent {
            name: "Nash".to_string(),
            status: PrimaryStatus::Running,
            description: Some("child-a".to_string()),
            started_at: chrono::Local::now(),
            ended_at: None,
        },
    });

    state.handle_event(AgentEvent::SubagentCompleted {
        id,
        pid: Some(44),
        subagent_name: "child-a".to_string(),
    });

    assert_eq!(state.agents[0].subagents.len(), 1);
    assert_eq!(state.agents[0].subagents[0].name, "Nash");
    assert_eq!(state.agents[0].subagents[0].status, PrimaryStatus::Done);
}

#[test]
fn test_agent_identity_session_has_no_pid() {
    let id = AgentId::session(AgentTool::Codex, "session-a");
    assert_eq!(
        id.identity,
        AgentIdentity::Session {
            tool: AgentTool::Codex,
            session_id: "session-a".to_string(),
        }
    );
    assert_eq!(id.pid(), None);
}

#[test]
fn test_claude_session_info_stays_process_keyed() {
    let mut state = make_state();
    state.handle_event(discovered(
        AgentTool::Claude,
        44,
        PathBuf::from("/tmp/test3"),
        None,
    ));

    state.handle_event(session_info(
        AgentTool::Claude,
        44,
        "claude-session-a".to_string(),
        None,
        false,
        None,
    ));
    state.handle_event(session_info(
        AgentTool::Claude,
        44,
        "claude-session-b".to_string(),
        None,
        false,
        None,
    ));

    assert_eq!(state.agents.len(), 1);
    assert_eq!(state.agents[0].pid.unwrap(), 44);
    assert_eq!(
        state.agents[0].session_id,
        Some("claude-session-b".to_string())
    );
}

#[test]
fn test_mixed_process_and_session_identities_stay_independent() {
    let mut state = make_state();

    state.handle_event(discovered(
        AgentTool::Claude,
        44,
        PathBuf::from("/tmp/claude"),
        None,
    ));
    state.handle_event(session_info(
        AgentTool::Codex,
        55,
        "codex-session-a".to_string(),
        None,
        false,
        None,
    ));

    assert_eq!(state.agents.len(), 2);
    assert_eq!(state.visible_agents().len(), 2);

    state.selected = Some(1);
    state.start_rename();
    assert_eq!(state.rename_input, "codex");
    state.rename_input = "codex-session".to_string();
    state.confirm_rename();

    let claude = state
        .agents
        .iter()
        .find(|agent| agent.id.tool() == AgentTool::Claude)
        .unwrap();
    let codex = state
        .agents
        .iter()
        .find(|agent| agent.id.is_session())
        .unwrap();
    assert_eq!(claude.alias, None);
    assert_eq!(codex.alias.as_deref(), Some("codex-session"));
    assert_eq!(codex.display_name(), "codex-session");

    state.handle_event(status_changed(
        AgentTool::Claude,
        44,
        PrimaryStatus::Running,
        PrimaryStatus::Done,
    ));

    let claude = state
        .agents
        .iter()
        .find(|agent| agent.id.tool() == AgentTool::Claude)
        .unwrap();
    let codex = state
        .agents
        .iter()
        .find(|agent| agent.id.is_session())
        .unwrap();
    assert_eq!(claude.status, PrimaryStatus::Done);
    assert_eq!(codex.status, PrimaryStatus::Running);
}

#[test]
fn test_default_sort_mode_is_name() {
    let state = make_state();
    assert_eq!(state.sort_mode, SortMode::Name);
}

#[test]
fn test_session_info_none_start_time_keeps_existing() {
    let mut state = make_state();
    let past = chrono::Local::now() - chrono::Duration::hours(3);
    state.handle_event(discovered(
        AgentTool::Codex,
        45,
        PathBuf::from("/tmp/test4"),
        Some(past),
    ));
    state.handle_event(session_info(
        AgentTool::Codex,
        45,
        "s2".to_string(),
        None,
        false,
        None,
    ));
    assert_eq!(state.agents[0].started_at, past);
}

#[test]
fn test_start_rename_sets_mode() {
    let mut state = make_state();
    state.handle_event(discovered(
        AgentTool::Claude,
        100,
        PathBuf::from("/tmp/a"),
        None,
    ));
    state.selected = Some(0);
    state.start_rename();
    assert!(state.rename_mode);
    assert_eq!(state.rename_input, "claude");
}

#[test]
fn test_confirm_rename_sets_alias() {
    let mut state = make_state();
    state.handle_event(discovered(
        AgentTool::Claude,
        100,
        PathBuf::from("/tmp/a"),
        None,
    ));
    state.selected = Some(0);
    state.start_rename();
    state.rename_input = "my-claude".to_string();
    state.confirm_rename();
    assert!(!state.rename_mode);
    assert_eq!(state.agents[0].alias, Some("my-claude".to_string()));
    assert_eq!(state.agents[0].display_name(), "my-claude");
}

#[test]
fn test_confirm_rename_empty_clears_alias() {
    let mut state = make_state();
    state.handle_event(discovered(
        AgentTool::Claude,
        100,
        PathBuf::from("/tmp/a"),
        None,
    ));
    state.selected = Some(0);
    state.agents[0].alias = Some("old-name".to_string());
    state.start_rename();
    state.rename_input.clear();
    state.confirm_rename();
    assert_eq!(state.agents[0].alias, None);
    assert_eq!(state.agents[0].display_name(), "claude");
}

#[test]
fn test_cancel_rename_preserves_alias() {
    let mut state = make_state();
    state.handle_event(discovered(
        AgentTool::Claude,
        100,
        PathBuf::from("/tmp/a"),
        None,
    ));
    state.selected = Some(0);
    state.agents[0].alias = Some("keep-this".to_string());
    state.start_rename();
    state.rename_input = "nope".to_string();
    state.cancel_rename();
    assert!(!state.rename_mode);
    assert_eq!(state.agents[0].alias, Some("keep-this".to_string()));
}

#[test]
fn test_start_rename_no_selection() {
    let mut state = make_state();
    state.selected = None;
    state.start_rename();
    assert!(!state.rename_mode);
}

#[test]
fn test_todos_updated_sets_todos() {
    let mut state = make_state();
    state.handle_event(discovered(
        AgentTool::Claude,
        100,
        PathBuf::from("/tmp"),
        None,
    ));
    let todos = vec![
        TodoItem {
            content: "Fix bug".to_string(),
            status: TodoStatus::InProgress,
            priority: TodoPriority::High,
        },
        TodoItem {
            content: "Write tests".to_string(),
            status: TodoStatus::Pending,
            priority: TodoPriority::High,
        },
    ];
    state.handle_event(todos_updated(AgentTool::Claude, 100, todos.clone()));
    assert_eq!(state.agents[0].todos, todos);
}

#[test]
fn test_todos_updated_replaces_previous() {
    let mut state = make_state();
    state.handle_event(discovered(
        AgentTool::Claude,
        100,
        PathBuf::from("/tmp"),
        None,
    ));
    let todos_v1 = vec![TodoItem {
        content: "Task A".to_string(),
        status: TodoStatus::Pending,
        priority: TodoPriority::Medium,
    }];
    state.handle_event(todos_updated(AgentTool::Claude, 100, todos_v1));
    let todos_v2 = vec![
        TodoItem {
            content: "Task A".to_string(),
            status: TodoStatus::Completed,
            priority: TodoPriority::Medium,
        },
        TodoItem {
            content: "Task B".to_string(),
            status: TodoStatus::InProgress,
            priority: TodoPriority::High,
        },
    ];
    state.handle_event(todos_updated(AgentTool::Claude, 100, todos_v2.clone()));
    assert_eq!(state.agents[0].todos, todos_v2);
}

#[test]
fn test_todos_updated_ignores_unknown_pid() {
    let mut state = make_state();
    state.handle_event(discovered(
        AgentTool::Claude,
        100,
        PathBuf::from("/tmp"),
        None,
    ));
    state.handle_event(todos_updated(
        AgentTool::Claude,
        999,
        vec![TodoItem {
            content: "Ghost".to_string(),
            status: TodoStatus::Pending,
            priority: TodoPriority::Low,
        }],
    ));
    assert!(state.agents[0].todos.is_empty());
}

// ── Fix #12: status count cache ───────────────────────────────────────────────

#[test]
fn test_count_by_status_zero_initially() {
    let state = make_state();
    assert_eq!(state.count_by_status(PrimaryStatus::Running), 0);
    assert_eq!(state.count_by_status(PrimaryStatus::Waiting), 0);
    assert_eq!(state.count_by_status(PrimaryStatus::Exited), 0);
    assert_eq!(state.count_by_status(PrimaryStatus::Done), 0);
}

#[test]
fn test_count_by_status_increments_on_discovered() {
    let mut state = make_state();
    state.handle_event(discovered(AgentTool::OpenCode, 1, "/tmp/a", None));
    assert_eq!(state.count_by_status(PrimaryStatus::Running), 1);

    state.handle_event(discovered(AgentTool::Claude, 2, "/tmp/b", None));
    assert_eq!(state.count_by_status(PrimaryStatus::Running), 2);
}

#[test]
fn test_count_by_status_updates_on_status_changed() {
    let mut state = make_state();
    state.handle_event(discovered(AgentTool::OpenCode, 1, "/tmp/a", None));
    state.handle_event(discovered(AgentTool::OpenCode, 2, "/tmp/b", None));

    state.handle_event(status_changed(
        AgentTool::OpenCode,
        1,
        PrimaryStatus::Running,
        PrimaryStatus::Waiting,
    ));

    assert_eq!(state.count_by_status(PrimaryStatus::Running), 1);
    assert_eq!(state.count_by_status(PrimaryStatus::Waiting), 1);
}

#[test]
fn test_count_by_status_updates_on_process_exited() {
    let mut state = make_state();
    state.handle_event(discovered(AgentTool::OpenCode, 1, "/tmp/a", None));
    state.handle_event(discovered(AgentTool::OpenCode, 2, "/tmp/b", None));
    state.handle_event(process_exited(AgentTool::OpenCode, 1));

    // OpenCode process agents are removed on exit (not just marked Exited)
    assert_eq!(state.count_by_status(PrimaryStatus::Running), 1);
    assert_eq!(state.count_by_status(PrimaryStatus::Exited), 0);
}

#[test]
fn test_count_by_status_consistent_with_agents_vec() {
    let mut state = make_state();
    state.handle_event(discovered(AgentTool::OpenCode, 1, "/tmp/a", None));
    state.handle_event(discovered(AgentTool::OpenCode, 2, "/tmp/b", None));
    state.handle_event(discovered(AgentTool::Claude, 3, "/tmp/c", None));
    state.handle_event(status_changed(
        AgentTool::OpenCode,
        1,
        PrimaryStatus::Running,
        PrimaryStatus::Waiting,
    ));
    state.handle_event(process_exited(AgentTool::OpenCode, 2));

    for status in [
        PrimaryStatus::Running,
        PrimaryStatus::Waiting,
        PrimaryStatus::Exited,
        PrimaryStatus::Done,
    ] {
        let expected = state.agents.iter().filter(|a| a.status == status).count();
        assert_eq!(
            state.count_by_status(status),
            expected,
            "count_by_status({status:?}) mismatch"
        );
    }
}

#[test]
fn codex_tab_active_profile_can_be_set() {
    let mut state = AppState::new(Config::default());
    state.accounts_tab.codex_profile_ids = vec!["profile-01".into(), "profile-02".into()];
    state.set_active_codex_profile(Some("profile-01".into()));
    assert_eq!(state.active_codex_profile_id(), Some("profile-01"));
}
