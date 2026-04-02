use ai_overview::types::*;
use ai_overview::types::{TodoItem, TodoPriority, TodoStatus};

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
        alias: None,
        todos: Vec::new(),
    };
    let elapsed = agent.elapsed();
    assert!(elapsed >= chrono::Duration::minutes(4));
    assert!(elapsed <= chrono::Duration::minutes(6));
}

// ── Timestamp conversion helper tests ─────────────────────────────

#[test]
fn test_from_epoch_millis_zero_returns_none() {
    assert!(from_epoch_millis(0).is_none());
}

#[test]
fn test_from_epoch_millis_valid() {
    // 2026-01-01T00:00:00Z = 1767225600000 ms
    let result = from_epoch_millis(1_767_225_600_000);
    assert!(result.is_some());
}

#[test]
fn test_from_epoch_millis_negative_is_valid() {
    // Negative timestamps (before 1970) are valid
    let result = from_epoch_millis(-1);
    assert!(result.is_some());
}

#[test]
fn test_from_epoch_secs_zero_returns_none() {
    assert!(from_epoch_secs(0).is_none());
}

#[test]
fn test_from_epoch_secs_valid() {
    // 2026-01-01T00:00:00Z = 1767225600 sec
    let result = from_epoch_secs(1_767_225_600);
    assert!(result.is_some());
}

#[test]
fn test_from_rfc3339_valid() {
    let result = from_rfc3339("2026-03-31T10:00:00Z");
    assert!(result.is_some());
}

#[test]
fn test_from_rfc3339_invalid_returns_none() {
    assert!(from_rfc3339("not-a-date").is_none());
    assert!(from_rfc3339("").is_none());
}

#[test]
fn test_activity_compacting_as_str() {
    assert_eq!(Activity::Compacting.as_str(), "compacting");
    assert_eq!(format!("{}", Activity::Compacting), "compacting");
}

#[test]
fn test_display_name_without_alias() {
    let agent = Agent {
        id: AgentId::new(AgentTool::Claude, 100),
        status: PrimaryStatus::Running,
        activity: None,
        cwd: std::path::PathBuf::from("/tmp"),
        branch: None,
        is_worktree: false,
        started_at: chrono::Local::now(),
        last_activity: chrono::Local::now(),
        subagents: Vec::new(),
        recent_events: std::collections::VecDeque::new(),
        session_id: None,
        alias: None,
        todos: Vec::new(),
    };
    assert_eq!(agent.display_name(), "claude");
}

#[test]
fn test_display_name_with_alias() {
    let agent = Agent {
        id: AgentId::new(AgentTool::OpenCode, 200),
        status: PrimaryStatus::Running,
        activity: None,
        cwd: std::path::PathBuf::from("/tmp"),
        branch: None,
        is_worktree: false,
        started_at: chrono::Local::now(),
        last_activity: chrono::Local::now(),
        subagents: Vec::new(),
        recent_events: std::collections::VecDeque::new(),
        session_id: None,
        alias: Some("my-agent".to_string()),
        todos: Vec::new(),
    };
    assert_eq!(agent.display_name(), "my-agent");
}

#[test]
fn test_sort_mode_cycling() {
    assert_eq!(SortMode::Urgency.next(), SortMode::Name);
    assert_eq!(SortMode::Name.next(), SortMode::Elapsed);
    assert_eq!(SortMode::Elapsed.next(), SortMode::Urgency);
}

#[test]
fn test_todo_status_equality() {
    assert_eq!(TodoStatus::Pending, TodoStatus::Pending);
    assert_ne!(TodoStatus::Pending, TodoStatus::InProgress);
    assert_ne!(TodoStatus::Completed, TodoStatus::Cancelled);
}

#[test]
fn test_todo_priority_equality() {
    assert_eq!(TodoPriority::High, TodoPriority::High);
    assert_ne!(TodoPriority::High, TodoPriority::Low);
}

#[test]
fn test_todo_item_equality() {
    let a = TodoItem {
        content: "Fix bug".to_string(),
        status: TodoStatus::Pending,
        priority: TodoPriority::High,
    };
    let b = TodoItem {
        content: "Fix bug".to_string(),
        status: TodoStatus::Pending,
        priority: TodoPriority::High,
    };
    let c = TodoItem {
        content: "Fix bug".to_string(),
        status: TodoStatus::Completed,
        priority: TodoPriority::High,
    };
    assert_eq!(a, b);
    assert_ne!(a, c);
}

#[test]
fn test_todo_status_from_str() {
    assert_eq!(TodoStatus::parse_str("pending"), TodoStatus::Pending);
    assert_eq!(TodoStatus::parse_str("in_progress"), TodoStatus::InProgress);
    assert_eq!(TodoStatus::parse_str("completed"), TodoStatus::Completed);
    assert_eq!(TodoStatus::parse_str("cancelled"), TodoStatus::Cancelled);
    assert_eq!(TodoStatus::parse_str("unknown_value"), TodoStatus::Pending);
}

#[test]
fn test_todo_priority_from_str() {
    assert_eq!(TodoPriority::parse_str("high"), TodoPriority::High);
    assert_eq!(TodoPriority::parse_str("medium"), TodoPriority::Medium);
    assert_eq!(TodoPriority::parse_str("low"), TodoPriority::Low);
    assert_eq!(
        TodoPriority::parse_str("unknown_value"),
        TodoPriority::Medium
    );
}
