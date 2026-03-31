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
    assert_eq!(state.sort_mode, SortMode::Urgency);
    assert!(!state.group_by_worktree);
    assert!(!state.show_help);
}

#[test]
fn test_discover_agent() {
    let mut state = make_state();
    state.handle_event(AgentEvent::Discovered {
        tool: AgentTool::Claude,
        pid: 1234,
        cwd: PathBuf::from("/tmp/project"),
    });
    assert_eq!(state.agents.len(), 1);
    assert_eq!(state.agents[0].pid, 1234);
    assert_eq!(state.agents[0].tool, AgentTool::Claude);
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
    });
    state.handle_event(AgentEvent::Discovered {
        tool: AgentTool::Claude,
        pid: 1234,
        cwd: PathBuf::from("/tmp/project"),
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
    });
    state.handle_event(AgentEvent::Discovered {
        tool: AgentTool::OpenCode,
        pid: 2,
        cwd: PathBuf::from("/b"),
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
    assert_eq!(state.sort_mode, SortMode::Urgency);
    state.cycle_sort_mode();
    assert_eq!(state.sort_mode, SortMode::Name);
    state.cycle_sort_mode();
    assert_eq!(state.sort_mode, SortMode::Elapsed);
    state.cycle_sort_mode();
    assert_eq!(state.sort_mode, SortMode::Urgency);
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
    assert_eq!(visible[0].pid, 3); // Waiting
    assert_eq!(visible[1].pid, 2); // Running
    assert_eq!(visible[2].pid, 1); // Done
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
        });
    }
    let visible = state.visible_agents();
    // Alphabetical: claude < codex < opencode
    assert_eq!(visible[0].tool, AgentTool::Claude);
    assert_eq!(visible[1].tool, AgentTool::Codex);
    assert_eq!(visible[2].tool, AgentTool::OpenCode);
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
        });
    }
    // Filter for "claude"
    state.filter = Some("claude".to_string());
    let visible = state.visible_agents();
    assert_eq!(visible.len(), 1);
    assert_eq!(visible[0].tool, AgentTool::Claude);

    // Filter for status "RUNNING" — should match all
    state.filter = Some("running".to_string());
    let visible = state.visible_agents();
    assert_eq!(visible.len(), 3);

    // Filter for something that matches nothing
    state.filter = Some("zzz".to_string());
    let visible = state.visible_agents();
    assert_eq!(visible.len(), 0);
}
