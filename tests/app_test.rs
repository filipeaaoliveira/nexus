use ai_overview::app::AppState;
use ai_overview::config::Config;
use ai_overview::event::AgentEvent;
use ai_overview::types::*;
use std::path::PathBuf;

fn make_state() -> AppState {
    AppState::new(Config::default())
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
fn test_discover_agent() {
    let mut state = make_state();
    state.handle_event(AgentEvent::Discovered {
        tool: AgentTool::Claude,
        pid: 1234,
        cwd: PathBuf::from("/tmp/project"),
        started_at: None,
    });
    assert_eq!(state.agents.len(), 1);
    assert_eq!(state.agents[0].id.pid, 1234);
    assert_eq!(state.agents[0].id.tool, AgentTool::Claude);
    assert_eq!(state.agents[0].status, PrimaryStatus::Running);
    assert_eq!(state.selected, Some(0));
}

#[test]
fn test_discover_duplicate_ignored() {
    let mut state = make_state();
    state.handle_event(AgentEvent::Discovered {
        tool: AgentTool::Claude,
        pid: 1234,
        cwd: PathBuf::from("/tmp/project"),
        started_at: None,
    });
    state.handle_event(AgentEvent::Discovered {
        tool: AgentTool::Claude,
        pid: 1234,
        cwd: PathBuf::from("/tmp/project"),
        started_at: None,
    });
    assert_eq!(state.agents.len(), 1);
}

#[test]
fn test_status_change() {
    let mut state = make_state();
    state.handle_event(AgentEvent::Discovered {
        tool: AgentTool::OpenCode,
        pid: 5678,
        cwd: PathBuf::from("/tmp/oc"),
        started_at: None,
    });
    state.handle_event(AgentEvent::StatusChanged {
        pid: 5678,
        tool: AgentTool::OpenCode,
        old: PrimaryStatus::Running,
        new: PrimaryStatus::Waiting,
    });
    assert_eq!(state.agents[0].status, PrimaryStatus::Waiting);
}

#[test]
fn test_process_exited() {
    let mut state = make_state();
    state.handle_event(AgentEvent::Discovered {
        tool: AgentTool::Codex,
        pid: 9999,
        cwd: PathBuf::from("/tmp/cx"),
        started_at: None,
    });
    state.handle_event(AgentEvent::ProcessExited {
        pid: 9999,
        tool: AgentTool::Codex,
    });
    assert_eq!(state.agents[0].status, PrimaryStatus::Exited);
}

#[test]
fn test_subagent_spawned_and_completed() {
    let mut state = make_state();
    state.handle_event(AgentEvent::Discovered {
        tool: AgentTool::OpenCode,
        pid: 1000,
        cwd: PathBuf::from("/tmp"),
        started_at: None,
    });
    state.handle_event(AgentEvent::SubagentSpawned {
        pid: 1000,
        tool: AgentTool::OpenCode,
        subagent: Subagent {
            name: "explore".to_string(),
            status: PrimaryStatus::Running,
            description: Some("searching codebase".to_string()),
            started_at: chrono::Local::now(),
            ended_at: None,
        },
    });
    assert_eq!(state.agents[0].subagents.len(), 1);
    assert_eq!(state.agents[0].subagents[0].name, "explore");
    state.handle_event(AgentEvent::SubagentCompleted {
        pid: 1000,
        tool: AgentTool::OpenCode,
        subagent_name: "explore".to_string(),
    });
    assert_eq!(state.agents[0].subagents[0].status, PrimaryStatus::Done);
    assert!(state.agents[0].subagents[0].ended_at.is_some());
}

#[test]
fn test_move_selection() {
    let mut state = make_state();
    for pid in [100, 200, 300] {
        state.handle_event(AgentEvent::Discovered {
            tool: AgentTool::Claude,
            pid,
            cwd: PathBuf::from(format!("/tmp/{pid}")),
            started_at: None,
        });
    }
    assert_eq!(state.selected, Some(0));
    state.move_selection_down();
    assert_eq!(state.selected, Some(1));
    state.move_selection_down();
    assert_eq!(state.selected, Some(2));
    state.move_selection_down();
    assert_eq!(state.selected, Some(2));
    state.move_selection_up();
    assert_eq!(state.selected, Some(1));
    state.move_selection_up();
    assert_eq!(state.selected, Some(0));
    state.move_selection_up();
    assert_eq!(state.selected, Some(0));
}

#[test]
fn test_count_by_status() {
    let mut state = make_state();
    state.handle_event(AgentEvent::Discovered {
        tool: AgentTool::Claude,
        pid: 1,
        cwd: PathBuf::from("/a"),
        started_at: None,
    });
    state.handle_event(AgentEvent::Discovered {
        tool: AgentTool::OpenCode,
        pid: 2,
        cwd: PathBuf::from("/b"),
        started_at: None,
    });
    state.handle_event(AgentEvent::StatusChanged {
        pid: 1,
        tool: AgentTool::Claude,
        old: PrimaryStatus::Running,
        new: PrimaryStatus::Waiting,
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
        state.handle_event(AgentEvent::Discovered {
            tool,
            pid,
            cwd: PathBuf::from(format!("/tmp/{pid}")),
            started_at: None,
        });
    }
    // Make agent 3 Waiting, agent 1 Done
    state.handle_event(AgentEvent::StatusChanged {
        pid: 3,
        tool: AgentTool::Codex,
        old: PrimaryStatus::Running,
        new: PrimaryStatus::Waiting,
    });
    state.handle_event(AgentEvent::StatusChanged {
        pid: 1,
        tool: AgentTool::Claude,
        old: PrimaryStatus::Running,
        new: PrimaryStatus::Done,
    });
    // Urgency sort: Waiting < Running < Done
    let visible = state.visible_agents();
    assert_eq!(visible[0].id.pid, 3); // Waiting
    assert_eq!(visible[1].id.pid, 2); // Running
    assert_eq!(visible[2].id.pid, 1); // Done
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
        state.handle_event(AgentEvent::Discovered {
            tool,
            pid,
            cwd: PathBuf::from(format!("/tmp/{pid}")),
            started_at: None,
        });
    }
    let visible = state.visible_agents();
    // Alphabetical: claude < codex < opencode
    assert_eq!(visible[0].id.tool, AgentTool::Claude);
    assert_eq!(visible[1].id.tool, AgentTool::Codex);
    assert_eq!(visible[2].id.tool, AgentTool::OpenCode);
}

#[test]
fn test_visible_agents_filter() {
    let mut state = make_state();
    for (pid, tool) in [
        (1, AgentTool::Claude),
        (2, AgentTool::OpenCode),
        (3, AgentTool::Codex),
    ] {
        state.handle_event(AgentEvent::Discovered {
            tool,
            pid,
            cwd: PathBuf::from(format!("/tmp/{pid}")),
            started_at: None,
        });
    }
    // Filter for "claude"
    state.filter = Some("claude".to_string());
    let visible = state.visible_agents();
    assert_eq!(visible.len(), 1);
    assert_eq!(visible[0].id.tool, AgentTool::Claude);

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
    state.handle_event(AgentEvent::Discovered {
        tool: AgentTool::Claude,
        pid: 1,
        cwd: PathBuf::from("/a"),
        started_at: None,
    });
    state.handle_event(AgentEvent::Discovered {
        tool: AgentTool::OpenCode,
        pid: 2,
        cwd: PathBuf::from("/b"),
        started_at: None,
    });
    state.handle_event(AgentEvent::Discovered {
        tool: AgentTool::Codex,
        pid: 3,
        cwd: PathBuf::from("/c"),
        started_at: None,
    });

    state.filter = Some("claude".to_string());
    let visible = state.visible_agents();
    assert_eq!(visible.len(), 1);
    assert_eq!(visible[0].id.tool, AgentTool::Claude);
}

#[test]
fn test_filter_by_branch() {
    let mut state = make_state();
    state.handle_event(AgentEvent::Discovered {
        tool: AgentTool::Claude,
        pid: 1,
        cwd: PathBuf::from("/a"),
        started_at: None,
    });
    state.handle_event(AgentEvent::SessionInfo {
        pid: 1,
        tool: AgentTool::Claude,
        session_id: "s1".to_string(),
        branch: Some("feat/auth".to_string()),
        is_worktree: false,
        started_at: None,
    });
    state.handle_event(AgentEvent::Discovered {
        tool: AgentTool::OpenCode,
        pid: 2,
        cwd: PathBuf::from("/b"),
        started_at: None,
    });
    state.handle_event(AgentEvent::SessionInfo {
        pid: 2,
        tool: AgentTool::OpenCode,
        session_id: "s2".to_string(),
        branch: Some("main".to_string()),
        is_worktree: false,
        started_at: None,
    });

    state.filter = Some("auth".to_string());
    let visible = state.visible_agents();
    assert_eq!(visible.len(), 1);
    assert_eq!(visible[0].id.pid, 1);
}

#[test]
fn test_filter_by_status() {
    let mut state = make_state();
    state.handle_event(AgentEvent::Discovered {
        tool: AgentTool::Claude,
        pid: 1,
        cwd: PathBuf::from("/a"),
        started_at: None,
    });
    state.handle_event(AgentEvent::StatusChanged {
        pid: 1,
        tool: AgentTool::Claude,
        old: PrimaryStatus::Running,
        new: PrimaryStatus::Waiting,
    });
    state.handle_event(AgentEvent::Discovered {
        tool: AgentTool::OpenCode,
        pid: 2,
        cwd: PathBuf::from("/b"),
        started_at: None,
    });

    state.filter = Some("waiting".to_string());
    let visible = state.visible_agents();
    assert_eq!(visible.len(), 1);
    assert_eq!(visible[0].id.pid, 1);
}

#[test]
fn test_filter_clamps_selected_index() {
    let mut state = make_state();
    // Add 4 agents
    for pid in [1, 2, 3, 4] {
        state.handle_event(AgentEvent::Discovered {
            tool: AgentTool::Claude,
            pid,
            cwd: PathBuf::from(format!("/tmp/{pid}")),
            started_at: None,
        });
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
    state.handle_event(AgentEvent::Discovered {
        tool: AgentTool::OpenCode,
        pid: 42,
        cwd: PathBuf::from("/tmp/test"),
        started_at: Some(past),
    });
    // The agent's started_at should be the past time, not now
    assert_eq!(state.agents[0].started_at, past);
}

#[test]
fn test_discover_without_start_time_uses_now() {
    let before = chrono::Local::now();
    let mut state = make_state();
    state.handle_event(AgentEvent::Discovered {
        tool: AgentTool::Claude,
        pid: 43,
        cwd: PathBuf::from("/tmp/test2"),
        started_at: None,
    });
    let after = chrono::Local::now();
    assert!(state.agents[0].started_at >= before);
    assert!(state.agents[0].started_at <= after);
}

#[test]
fn test_remove_exited_agents() {
    let mut state = make_state();
    state.handle_event(AgentEvent::Discovered {
        tool: AgentTool::Claude,
        pid: 100,
        cwd: "/tmp/a".into(),
        started_at: None,
    });
    state.handle_event(AgentEvent::Discovered {
        tool: AgentTool::OpenCode,
        pid: 200,
        cwd: "/tmp/b".into(),
        started_at: None,
    });
    state.handle_event(AgentEvent::Discovered {
        tool: AgentTool::Codex,
        pid: 300,
        cwd: "/tmp/c".into(),
        started_at: None,
    });
    // Exit agent 100 and 300
    state.handle_event(AgentEvent::ProcessExited {
        pid: 100,
        tool: AgentTool::Claude,
    });
    state.handle_event(AgentEvent::ProcessExited {
        pid: 300,
        tool: AgentTool::Codex,
    });
    state.selected = Some(0);
    state.remove_exited();
    assert_eq!(state.agents.len(), 1);
    assert_eq!(state.agents[0].id.pid, 200);
    assert_eq!(state.selected, Some(0));
}

#[test]
fn test_remove_exited_no_exited_agents() {
    let mut state = make_state();
    state.handle_event(AgentEvent::Discovered {
        tool: AgentTool::Claude,
        pid: 100,
        cwd: "/tmp/a".into(),
        started_at: None,
    });
    state.selected = Some(0);
    state.remove_exited();
    assert_eq!(state.agents.len(), 1); // no change
    assert_eq!(state.selected, Some(0));
}

#[test]
fn test_remove_exited_all_exited() {
    let mut state = make_state();
    state.handle_event(AgentEvent::Discovered {
        tool: AgentTool::Claude,
        pid: 100,
        cwd: "/tmp/a".into(),
        started_at: None,
    });
    state.handle_event(AgentEvent::ProcessExited {
        pid: 100,
        tool: AgentTool::Claude,
    });
    state.selected = Some(0);
    state.remove_exited();
    assert_eq!(state.agents.len(), 0);
    assert_eq!(state.selected, None);
}

#[test]
fn test_remove_selected_agent() {
    let mut state = make_state();
    state.handle_event(AgentEvent::Discovered {
        tool: AgentTool::Claude,
        pid: 100,
        cwd: "/tmp/a".into(),
        started_at: None,
    });
    state.handle_event(AgentEvent::Discovered {
        tool: AgentTool::OpenCode,
        pid: 200,
        cwd: "/tmp/b".into(),
        started_at: None,
    });
    assert_eq!(state.agents.len(), 2);
    state.selected = Some(0);
    state.remove_selected();
    assert_eq!(state.agents.len(), 1);
    assert_eq!(state.agents[0].id.pid, 200);
    assert_eq!(state.selected, Some(0));
}

#[test]
fn test_remove_selected_last_item_clamps() {
    let mut state = make_state();
    state.handle_event(AgentEvent::Discovered {
        tool: AgentTool::Claude,
        pid: 100,
        cwd: "/tmp/a".into(),
        started_at: None,
    });
    state.handle_event(AgentEvent::Discovered {
        tool: AgentTool::OpenCode,
        pid: 200,
        cwd: "/tmp/b".into(),
        started_at: None,
    });
    state.selected = Some(1);
    state.remove_selected();
    assert_eq!(state.agents.len(), 1);
    assert_eq!(state.selected, Some(0));
}

#[test]
fn test_remove_selected_only_item() {
    let mut state = make_state();
    state.handle_event(AgentEvent::Discovered {
        tool: AgentTool::Claude,
        pid: 100,
        cwd: "/tmp/a".into(),
        started_at: None,
    });
    state.selected = Some(0);
    state.remove_selected();
    assert_eq!(state.agents.len(), 0);
    assert_eq!(state.selected, None);
}

#[test]
fn test_remove_selected_none_selected() {
    let mut state = make_state();
    state.handle_event(AgentEvent::Discovered {
        tool: AgentTool::Claude,
        pid: 100,
        cwd: "/tmp/a".into(),
        started_at: None,
    });
    state.selected = None;
    state.remove_selected();
    assert_eq!(state.agents.len(), 1); // no change
}

#[test]
fn test_remove_selected_with_filter_removes_correct_agent() {
    let mut state = make_state();
    // Add claude (pid 100) and opencode (pid 200)
    state.handle_event(AgentEvent::Discovered {
        tool: AgentTool::Claude,
        pid: 100,
        cwd: "/tmp/a".into(),
        started_at: None,
    });
    state.handle_event(AgentEvent::Discovered {
        tool: AgentTool::OpenCode,
        pid: 200,
        cwd: "/tmp/b".into(),
        started_at: None,
    });
    // Filter to show only opencode
    state.filter = Some("opencode".to_string());
    state.selected = Some(0); // first visible = opencode
    state.remove_selected();
    // opencode should be removed, claude should remain
    assert_eq!(state.agents.len(), 1);
    assert_eq!(state.agents[0].id.tool, AgentTool::Claude);
}

#[test]
fn test_rename_with_filter_renames_correct_agent() {
    let mut state = make_state();
    // Add claude (pid 100) and opencode (pid 200)
    state.handle_event(AgentEvent::Discovered {
        tool: AgentTool::Claude,
        pid: 100,
        cwd: "/tmp/a".into(),
        started_at: None,
    });
    state.handle_event(AgentEvent::Discovered {
        tool: AgentTool::OpenCode,
        pid: 200,
        cwd: "/tmp/b".into(),
        started_at: None,
    });
    // Filter to show only opencode
    state.filter = Some("opencode".to_string());
    state.selected = Some(0); // first visible = opencode
    state.start_rename();
    state.rename_input = "my-oc".to_string();
    state.confirm_rename();
    // opencode should be renamed, claude untouched
    let oc = state.agents.iter().find(|a| a.id.pid == 200).unwrap();
    assert_eq!(oc.alias, Some("my-oc".to_string()));
    let cl = state.agents.iter().find(|a| a.id.pid == 100).unwrap();
    assert_eq!(cl.alias, None);
}

#[test]
fn test_session_info_only_applies_earlier_start_time() {
    let mut state = make_state();
    let now = chrono::Local::now();
    state.handle_event(AgentEvent::Discovered {
        tool: AgentTool::OpenCode,
        pid: 44,
        cwd: PathBuf::from("/tmp/test3"),
        started_at: None, // uses ~now
    });

    // SessionInfo with earlier time should update
    let earlier = now - chrono::Duration::hours(1);
    state.handle_event(AgentEvent::SessionInfo {
        pid: 44,
        tool: AgentTool::OpenCode,
        session_id: "s1".to_string(),
        branch: None,
        is_worktree: false,
        started_at: Some(earlier),
    });
    assert_eq!(state.agents[0].started_at, earlier);

    // SessionInfo with later time should NOT update
    let later = now + chrono::Duration::hours(1);
    state.handle_event(AgentEvent::SessionInfo {
        pid: 44,
        tool: AgentTool::OpenCode,
        session_id: "s1".to_string(),
        branch: None,
        is_worktree: false,
        started_at: Some(later),
    });
    // Should still be the earlier time
    assert_eq!(state.agents[0].started_at, earlier);
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
    state.handle_event(AgentEvent::Discovered {
        tool: AgentTool::Codex,
        pid: 45,
        cwd: PathBuf::from("/tmp/test4"),
        started_at: Some(past),
    });
    state.handle_event(AgentEvent::SessionInfo {
        pid: 45,
        tool: AgentTool::Codex,
        session_id: "s2".to_string(),
        branch: None,
        is_worktree: false,
        started_at: None,
    });
    assert_eq!(state.agents[0].started_at, past);
}

#[test]
fn test_start_rename_sets_mode() {
    let mut state = make_state();
    state.handle_event(AgentEvent::Discovered {
        tool: AgentTool::Claude,
        pid: 100,
        cwd: "/tmp/a".into(),
        started_at: None,
    });
    state.selected = Some(0);
    state.start_rename();
    assert!(state.rename_mode);
    assert_eq!(state.rename_input, "claude");
}

#[test]
fn test_confirm_rename_sets_alias() {
    let mut state = make_state();
    state.handle_event(AgentEvent::Discovered {
        tool: AgentTool::Claude,
        pid: 100,
        cwd: "/tmp/a".into(),
        started_at: None,
    });
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
    state.handle_event(AgentEvent::Discovered {
        tool: AgentTool::Claude,
        pid: 100,
        cwd: "/tmp/a".into(),
        started_at: None,
    });
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
    state.handle_event(AgentEvent::Discovered {
        tool: AgentTool::Claude,
        pid: 100,
        cwd: "/tmp/a".into(),
        started_at: None,
    });
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
    state.handle_event(AgentEvent::Discovered {
        tool: AgentTool::Claude,
        pid: 100,
        cwd: PathBuf::from("/tmp"),
        started_at: None,
    });
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
    state.handle_event(AgentEvent::TodosUpdated {
        pid: 100,
        tool: AgentTool::Claude,
        todos: todos.clone(),
    });
    assert_eq!(state.agents[0].todos, todos);
}

#[test]
fn test_todos_updated_replaces_previous() {
    let mut state = make_state();
    state.handle_event(AgentEvent::Discovered {
        tool: AgentTool::Claude,
        pid: 100,
        cwd: PathBuf::from("/tmp"),
        started_at: None,
    });
    let todos_v1 = vec![TodoItem {
        content: "Task A".to_string(),
        status: TodoStatus::Pending,
        priority: TodoPriority::Medium,
    }];
    state.handle_event(AgentEvent::TodosUpdated {
        pid: 100,
        tool: AgentTool::Claude,
        todos: todos_v1,
    });
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
    state.handle_event(AgentEvent::TodosUpdated {
        pid: 100,
        tool: AgentTool::Claude,
        todos: todos_v2.clone(),
    });
    assert_eq!(state.agents[0].todos, todos_v2);
}

#[test]
fn test_todos_updated_ignores_unknown_pid() {
    let mut state = make_state();
    state.handle_event(AgentEvent::Discovered {
        tool: AgentTool::Claude,
        pid: 100,
        cwd: PathBuf::from("/tmp"),
        started_at: None,
    });
    state.handle_event(AgentEvent::TodosUpdated {
        pid: 999,
        tool: AgentTool::Claude,
        todos: vec![TodoItem {
            content: "Ghost".to_string(),
            status: TodoStatus::Pending,
            priority: TodoPriority::Low,
        }],
    });
    assert!(state.agents[0].todos.is_empty());
}
