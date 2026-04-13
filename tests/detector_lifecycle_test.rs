use std::path::PathBuf;

use nexus::domain::agents::{AgentId, AgentTool};
use nexus::{AgentEvent, AppState, RootState};

fn discovered(tool: AgentTool, pid: u32, cwd: &str) -> AgentEvent {
    AgentEvent::Discovered {
        id: AgentId::new(tool, pid),
        pid: Some(pid),
        cwd: PathBuf::from(cwd),
        started_at: None,
    }
}

fn process_exited(tool: AgentTool, pid: u32) -> AgentEvent {
    AgentEvent::ProcessExited {
        id: AgentId::new(tool, pid),
        pid: Some(pid),
    }
}

#[test]
fn discovered_then_session_info_produces_one_logical_agent() {
    let mut app = AppState::from_root(RootState::default());

    app.handle_event(AgentEvent::Discovered {
        id: AgentId::new(AgentTool::Codex, 44),
        pid: Some(44),
        cwd: PathBuf::from("/tmp/repo"),
        started_at: None,
    });

    app.handle_event(AgentEvent::SessionInfo {
        id: AgentId::session(AgentTool::Codex, "thread-1"),
        pid: Some(44),
        session_id: "thread-1".into(),
        branch: Some("main".into()),
        is_worktree: false,
        started_at: None,
    });

    let visible = app.visible_agents();
    assert_eq!(visible.len(), 1);
    assert!(visible[0].id.is_session());
    assert_eq!(visible[0].pid, Some(44));
    assert_eq!(visible[0].cwd, PathBuf::from("/tmp/repo"));
}

#[test]
fn session_info_then_discovered_merges_into_existing_session_row() {
    let mut app = AppState::from_root(RootState::default());

    app.handle_event(AgentEvent::SessionInfo {
        id: AgentId::session(AgentTool::Codex, "thread-1"),
        pid: Some(44),
        session_id: "thread-1".into(),
        branch: Some("main".into()),
        is_worktree: false,
        started_at: None,
    });
    app.handle_event(discovered(AgentTool::Codex, 44, "/tmp/repo"));

    let visible = app.visible_agents();
    assert_eq!(visible.len(), 1);
    assert_eq!(
        visible[0].id,
        AgentId::session(AgentTool::Codex, "thread-1")
    );
    assert_eq!(visible[0].cwd, PathBuf::from("/tmp/repo"));
    assert_eq!(visible[0].session_id.as_deref(), Some("thread-1"));
}

#[test]
fn process_exit_removes_codex_process_only_rows_from_visibility() {
    let mut app = AppState::from_root(RootState::default());

    app.handle_event(discovered(AgentTool::Codex, 88, "/tmp/repo"));
    assert_eq!(app.visible_agents().len(), 1);

    app.handle_event(process_exited(AgentTool::Codex, 88));

    assert!(app.visible_agents().is_empty());
}
