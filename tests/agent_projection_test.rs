use std::path::PathBuf;

use nexus::domain::agents::{
    Activity, Agent, AgentId, AgentTool, NormalizedState, NormalizedStatus, PrimaryStatus,
};

#[test]
fn agent_id_projection_tracks_process_and_session_identity() {
    let process = AgentId::new(AgentTool::Claude, 100);
    let session = AgentId::session(AgentTool::Codex, "thread-1");

    assert!(process.is_process());
    assert!(!process.is_session());
    assert_eq!(process.pid(), Some(100));
    assert_eq!(process.tool(), AgentTool::Claude);

    assert!(session.is_session());
    assert!(!session.is_process());
    assert_eq!(session.session_id(), Some("thread-1"));
    assert_eq!(session.tool(), AgentTool::Codex);
}

#[test]
fn agent_display_projection_prefers_alias_over_tool_name() {
    let agent = Agent {
        id: AgentId::new(AgentTool::OpenCode, 7),
        pid: Some(7),
        status: PrimaryStatus::Running,
        normalized: NormalizedState::new(NormalizedStatus::Working, None),
        activity: Some(Activity::Generating),
        cwd: Some(PathBuf::from("/tmp/repo")),
        branch: Some("main".into()),
        is_worktree: false,
        started_at: chrono::Local::now(),
        last_activity: chrono::Local::now(),
        subagents: Vec::new(),
        recent_events: std::collections::VecDeque::new(),
        session_id: Some("thread-7".into()),
        alias: Some("focused-runner".into()),
        todos: Vec::new(),
    };

    assert_eq!(agent.display_name(), "focused-runner");
}
