use std::path::{Path, PathBuf};

use nexus::testing::{AgentEvent, AgentId, AgentTool, AppState, RootState};

fn discovered(pid: u32, cwd: &str) -> AgentEvent {
    AgentEvent::Discovered {
        id: AgentId::new(AgentTool::Codex, pid),
        pid: Some(pid),
        cwd: PathBuf::from(cwd),
        started_at: None,
    }
}

fn session_info(pid: Option<u32>, session_id: &str, branch: Option<&str>) -> AgentEvent {
    AgentEvent::SessionInfo {
        id: AgentId::session(AgentTool::Codex, session_id),
        pid,
        session_id: Some(session_id.to_string()),
        branch: branch.map(str::to_string),
        is_worktree: false,
        started_at: None,
    }
}

#[test]
fn discovered_then_session_info_produces_one_logical_agent() {
    let mut app = AppState::from_root(RootState::default());

    app.handle_event(discovered(44, "/tmp/repo"));
    app.handle_event(session_info(Some(44), "thread-1", Some("main")));

    let visible = app.visible_agents();
    assert_eq!(visible.len(), 1);
    assert_eq!(
        *visible[0].id(),
        AgentId::session(AgentTool::Codex, "thread-1")
    );
    assert_eq!(visible[0].pid(), Some(44));
    assert_eq!(visible[0].cwd(), Some(Path::new("/tmp/repo")));
}

#[test]
fn session_info_then_discovered_merges_into_existing_session_row() {
    let mut app = AppState::from_root(RootState::default());

    app.handle_event(session_info(Some(44), "thread-1", Some("main")));
    app.handle_event(discovered(44, "/tmp/repo"));

    let visible = app.visible_agents();
    assert_eq!(visible.len(), 1);
    assert_eq!(
        *visible[0].id(),
        AgentId::session(AgentTool::Codex, "thread-1")
    );
    assert_eq!(visible[0].pid(), Some(44));
    assert_eq!(visible[0].session_id(), Some("thread-1"));
    assert_eq!(visible[0].cwd(), Some(Path::new("/tmp/repo")));
}

#[test]
fn pidless_session_then_discovery_then_session_info_resolves_without_duplicate_row() {
    let mut app = AppState::from_root(RootState::default());

    app.handle_event(session_info(None, "thread-1", Some("main")));
    assert!(app.visible_agents().is_empty());

    app.handle_event(discovered(44, "/tmp/repo"));
    assert_eq!(app.visible_agents().len(), 1);
    assert_eq!(
        *app.visible_agents()[0].id(),
        AgentId::new(AgentTool::Codex, 44)
    );

    app.handle_event(session_info(Some(44), "thread-1", Some("main")));

    let visible = app.visible_agents();
    assert_eq!(visible.len(), 1);
    assert_eq!(
        *visible[0].id(),
        AgentId::session(AgentTool::Codex, "thread-1")
    );
    assert_eq!(visible[0].pid(), Some(44));
}

#[test]
fn process_exit_removes_codex_process_only_row_from_visibility() {
    let mut app = AppState::from_root(RootState::default());

    app.handle_event(discovered(88, "/tmp/repo"));
    assert_eq!(app.visible_agents().len(), 1);

    app.handle_event(AgentEvent::ProcessExited {
        id: AgentId::new(AgentTool::Codex, 88),
        pid: Some(88),
    });

    assert!(app.visible_agents().is_empty());
}

#[test]
fn process_exit_marks_session_bound_codex_agent_exited_without_creating_new_row() {
    let mut app = AppState::from_root(RootState::default());

    app.handle_event(session_info(Some(91), "thread-9", Some("main")));
    assert_eq!(app.visible_agents().len(), 1);

    app.handle_event(AgentEvent::ProcessExited {
        id: AgentId::new(AgentTool::Codex, 91),
        pid: Some(91),
    });

    let visible = app.visible_agents();
    assert_eq!(visible.len(), 1);
    assert_eq!(
        *visible[0].id(),
        AgentId::session(AgentTool::Codex, "thread-9")
    );
    assert_eq!(visible[0].pid(), Some(91));
    assert_eq!(visible[0].status().as_str(), "EXITED");
}
