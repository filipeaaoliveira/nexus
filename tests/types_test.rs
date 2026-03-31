use ai_overview::types::*;

#[test]
fn test_primary_status_ordering() {
    let mut statuses = vec![
        PrimaryStatus::Done,
        PrimaryStatus::Running,
        PrimaryStatus::Waiting,
        PrimaryStatus::Exited,
        PrimaryStatus::Error,
    ];
    statuses.sort();
    assert_eq!(
        statuses,
        vec![
            PrimaryStatus::Waiting,
            PrimaryStatus::Running,
            PrimaryStatus::Error,
            PrimaryStatus::Done,
            PrimaryStatus::Exited,
        ]
    );
}

#[test]
fn test_primary_status_display() {
    assert_eq!(PrimaryStatus::Waiting.as_str(), "WAITING");
    assert_eq!(PrimaryStatus::Running.as_str(), "RUNNING");
    assert_eq!(PrimaryStatus::Error.as_str(), "ERROR");
    assert_eq!(PrimaryStatus::Done.as_str(), "DONE");
    assert_eq!(PrimaryStatus::Exited.as_str(), "EXITED");
}

#[test]
fn test_agent_tool_display() {
    assert_eq!(AgentTool::OpenCode.as_str(), "opencode");
    assert_eq!(AgentTool::Claude.as_str(), "claude");
    assert_eq!(AgentTool::Codex.as_str(), "codex");
}

#[test]
fn test_agent_id_uniqueness() {
    let id1 = AgentId::new(AgentTool::OpenCode, 1234);
    let id2 = AgentId::new(AgentTool::OpenCode, 5678);
    let id3 = AgentId::new(AgentTool::Claude, 1234);
    assert_ne!(id1, id2);
    assert_ne!(id1, id3);
}

#[test]
fn test_agent_elapsed_time() {
    let agent = Agent {
        id: AgentId::new(AgentTool::Claude, 1000),
        tool: AgentTool::Claude,
        pid: 1000,
        status: PrimaryStatus::Running,
        activity: Some(Activity::Generating),
        cwd: std::path::PathBuf::from("/tmp/test"),
        branch: Some("main".to_string()),
        is_worktree: false,
        started_at: chrono::Local::now() - chrono::Duration::minutes(5),
        last_activity: chrono::Local::now(),
        subagents: vec![],
        recent_events: std::collections::VecDeque::new(),
        session_id: None,
    };
    let elapsed = agent.elapsed();
    assert!(elapsed >= chrono::Duration::minutes(4));
    assert!(elapsed <= chrono::Duration::minutes(6));
}

#[test]
fn test_sort_mode_cycling() {
    assert_eq!(SortMode::Urgency.next(), SortMode::Name);
    assert_eq!(SortMode::Name.next(), SortMode::Elapsed);
    assert_eq!(SortMode::Elapsed.next(), SortMode::Urgency);
}
