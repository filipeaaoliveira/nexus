use std::path::PathBuf;

use nexus::testing::{
    Activity, Agent, AgentDetail, AgentId, AgentIdent, AgentLifecycle, AgentTool, NormalizedState,
    NormalizedStatus, PrimaryStatus,
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
    let agent = Agent::new(
        AgentIdent::new(
            AgentId::new(AgentTool::OpenCode, 7),
            Some(7),
            Some("thread-7".into()),
            Some("focused-runner".into()),
            Some(PathBuf::from("/tmp/repo")),
            Some("main".into()),
            false,
            chrono::Local::now(),
        ),
        AgentLifecycle::new(
            PrimaryStatus::Running,
            NormalizedState::new(NormalizedStatus::Working, None),
            Some(Activity::Generating),
            chrono::Local::now(),
        ),
        AgentDetail::new(Vec::new(), std::collections::VecDeque::new(), Vec::new()),
    );

    assert_eq!(agent.display_name(), "focused-runner");
}
