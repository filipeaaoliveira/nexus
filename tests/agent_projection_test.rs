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

#[test]
fn overview_projection_tracks_visible_agents_without_join_drift() {
    let mut app = AppState::from_root(RootState::default());
    app.handle_event(discovered(AgentTool::Claude, 100, "/tmp/one"));
    app.handle_event(discovered(AgentTool::OpenCode, 200, "/tmp/two"));

    let overview_ids: Vec<_> = app
        .root()
        .overview()
        .visible_agents()
        .into_iter()
        .map(|agent| agent.id.clone())
        .collect();
    let visible_agents = app.visible_agents();
    let visible_ids: Vec<_> = visible_agents
        .iter()
        .map(|agent| agent.id.clone())
        .collect();

    assert_eq!(overview_ids, visible_ids);
    assert_eq!(visible_agents.len(), 2);
    assert_eq!(visible_agents[0].id.tool(), AgentTool::Claude);
    assert_eq!(visible_agents[1].id.tool(), AgentTool::OpenCode);
    assert_eq!(
        app.root().overview().selected_agent_id(),
        Some(visible_agents[0].id.clone())
    );
}

#[test]
fn codex_session_without_pid_stays_hidden_until_process_binding_exists() {
    let mut app = AppState::from_root(RootState::default());

    app.handle_event(AgentEvent::SessionInfo {
        id: AgentId::session(AgentTool::Codex, "thread-1"),
        pid: None,
        session_id: "thread-1".into(),
        branch: Some("main".into()),
        is_worktree: false,
        started_at: None,
    });

    assert!(app.visible_agents().is_empty());
    assert_eq!(app.root().overview().selected_agent_id(), None);

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
    assert_eq!(
        visible[0].id,
        AgentId::session(AgentTool::Codex, "thread-1")
    );
    assert_eq!(
        app.root().overview().selected_agent_id(),
        Some(visible[0].id.clone())
    );
}
