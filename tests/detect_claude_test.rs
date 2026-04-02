use ai_overview::detect::claude::*;
use ai_overview::types::{Activity, PrimaryStatus, TodoPriority, TodoStatus};

#[test]
fn test_infer_activity_tool_use_last() {
    let entries = vec![TranscriptEntry::ToolUse {
        timestamp: "2026-03-31T09:18:37.023Z".to_string(),
        tool_name: "read".to_string(),
        tool_input: serde_json::Map::new(),
    }];
    assert_eq!(
        infer_activity_from_last_entry(&entries),
        Some(ai_overview::types::Activity::ToolCall)
    );
}

#[test]
fn test_infer_activity_tool_result_last() {
    let content = include_str!("fixtures/claude_transcript_tool_result_last.jsonl");
    let entries = parse_transcript(content);
    assert_eq!(
        infer_activity_from_last_entry(&entries),
        Some(ai_overview::types::Activity::Thinking)
    );
}

#[test]
fn test_infer_activity_user_last() {
    let content = include_str!("fixtures/claude_transcript_user_last.jsonl");
    let entries = parse_transcript(content);
    assert_eq!(
        infer_activity_from_last_entry(&entries),
        Some(ai_overview::types::Activity::Generating)
    );
}

#[test]
fn test_infer_activity_empty() {
    assert_eq!(infer_activity_from_last_entry(&[]), None);
}

#[test]
fn test_parse_transcript_events() {
    let content = include_str!("fixtures/claude_transcript.jsonl");
    let events = parse_transcript(content);
    assert_eq!(events.len(), 9);
}

#[test]
fn test_extract_tool_calls() {
    let content = include_str!("fixtures/claude_transcript.jsonl");
    let events = parse_transcript(content);
    let tool_calls: Vec<_> = events
        .iter()
        .filter(|e| matches!(e, TranscriptEntry::ToolUse { .. }))
        .collect();
    assert_eq!(tool_calls.len(), 4); // glob, read, edit, task
}

#[test]
fn test_detect_subagent_spawn() {
    let content = include_str!("fixtures/claude_transcript.jsonl");
    let events = parse_transcript(content);
    let subagents: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            TranscriptEntry::ToolUse {
                tool_name,
                tool_input,
                ..
            } if tool_name == "task" => tool_input
                .get("subagent_type")
                .and_then(|v| v.as_str())
                .map(String::from),
            _ => None,
        })
        .collect();
    assert_eq!(subagents.len(), 1);
    assert_eq!(subagents[0], "explore");
}

#[test]
fn test_infer_status_active_tool_call() {
    let content = include_str!("fixtures/claude_transcript.jsonl");
    let events = parse_transcript(content);
    let status = infer_status_from_transcript(&events);
    assert_eq!(status, InferredStatus::Running);
}

#[test]
fn test_infer_status_from_empty() {
    let events = parse_transcript("");
    let status = infer_status_from_transcript(&events);
    assert_eq!(status, InferredStatus::Unknown);
}

#[test]
fn test_parse_project_session_extracts_branch() {
    let content = include_str!("fixtures/claude_project_session.jsonl");
    let info = parse_project_session(content);
    assert_eq!(info.session_id.as_deref(), Some("b1508a89-test"));
    assert_eq!(info.branch.as_deref(), Some("feat/error-handling"));
    assert_eq!(
        info.cwd.as_deref(),
        Some(std::path::Path::new("/Users/dev/project"))
    );
}

#[test]
fn test_detect_internal_initiator_subagent() {
    let content = include_str!("fixtures/claude_project_session.jsonl");
    let info = parse_project_session(content);
    assert!(info.has_internal_initiator);
}

/// Ensures that processing a transcript ending with tool_result does NOT
/// emit a ToolCallCompleted event (which would cause activity=Idle).
/// We verify this by checking that the last entry type after a tool_result
/// maps to Thinking, not to something that would produce ToolCallCompleted.
#[test]
fn test_tool_result_does_not_imply_idle_activity() {
    let content = include_str!("fixtures/claude_transcript_tool_result_last.jsonl");
    let entries = parse_transcript(content);
    // The activity inferred should be Thinking, never Idle
    let activity = infer_activity_from_last_entry(&entries);
    assert_ne!(activity, Some(ai_overview::types::Activity::Idle));
    assert_eq!(activity, Some(ai_overview::types::Activity::Thinking));
}

#[test]
fn test_inferred_to_primary_mapping() {
    assert_eq!(
        inferred_to_primary(InferredStatus::Running),
        PrimaryStatus::Running
    );
    assert_eq!(
        inferred_to_primary(InferredStatus::Waiting),
        PrimaryStatus::Waiting
    );
    assert_eq!(
        inferred_to_primary(InferredStatus::ToolCall),
        PrimaryStatus::Running
    );
    assert_eq!(
        inferred_to_primary(InferredStatus::Unknown),
        PrimaryStatus::Running
    );
}

#[test]
fn test_is_stale_old_mtime() {
    use std::time::{Duration, SystemTime};
    let old = SystemTime::now() - Duration::from_secs(30);
    assert!(ai_overview::detect::claude::is_stale(old, 10));
}

#[test]
fn test_is_stale_recent_mtime() {
    use std::time::SystemTime;
    let now = SystemTime::now();
    assert!(!ai_overview::detect::claude::is_stale(now, 10));
}

#[test]
fn test_infer_activity_compaction_entry() {
    let entries = vec![TranscriptEntry::User {
        timestamp: "2026-04-02T10:00:00Z".to_string(),
        content: "[restore checkpointed session agent configuration after compaction]".to_string(),
    }];
    let activity = infer_activity_from_last_entry(&entries);
    assert_eq!(activity, Some(Activity::Compacting));
}

#[test]
fn test_infer_activity_normal_user_entry_not_compaction() {
    let entries = vec![TranscriptEntry::User {
        timestamp: "2026-04-02T10:00:00Z".to_string(),
        content: "Please fix the bug in main.rs".to_string(),
    }];
    let activity = infer_activity_from_last_entry(&entries);
    assert_eq!(activity, Some(Activity::Generating));
}

// ── extract_todos_from_transcript tests ───────────────────────────

#[test]
fn test_extract_todos_empty_transcript() {
    let entries: Vec<TranscriptEntry> = vec![];
    let todos = extract_todos_from_transcript(&entries);
    assert!(todos.is_empty());
}

#[test]
fn test_extract_todos_no_todowrite_calls() {
    let entries = vec![
        TranscriptEntry::User {
            timestamp: "2026-04-02T10:00:00Z".to_string(),
            content: "hello".to_string(),
        },
        TranscriptEntry::ToolUse {
            timestamp: "2026-04-02T10:00:01Z".to_string(),
            tool_name: "bash".to_string(),
            tool_input: serde_json::Map::new(),
        },
    ];
    let todos = extract_todos_from_transcript(&entries);
    assert!(todos.is_empty());
}

#[test]
fn test_extract_todos_single_todowrite() {
    let mut input = serde_json::Map::new();
    input.insert(
        "todos".to_string(),
        serde_json::json!([
            {"content": "Fix bug", "status": "in_progress", "priority": "high"},
            {"content": "Write tests", "status": "pending", "priority": "medium"},
        ]),
    );
    let entries = vec![TranscriptEntry::ToolUse {
        timestamp: "2026-04-02T10:00:00Z".to_string(),
        tool_name: "todowrite".to_string(),
        tool_input: input,
    }];
    let todos = extract_todos_from_transcript(&entries);
    assert_eq!(todos.len(), 2);
    assert_eq!(todos[0].content, "Fix bug");
    assert_eq!(todos[0].status, TodoStatus::InProgress);
    assert_eq!(todos[0].priority, TodoPriority::High);
    assert_eq!(todos[1].content, "Write tests");
    assert_eq!(todos[1].status, TodoStatus::Pending);
    assert_eq!(todos[1].priority, TodoPriority::Medium);
}

#[test]
fn test_extract_todos_uses_last_todowrite() {
    let mut input1 = serde_json::Map::new();
    input1.insert(
        "todos".to_string(),
        serde_json::json!([
            {"content": "Old task", "status": "pending", "priority": "high"},
        ]),
    );
    let mut input2 = serde_json::Map::new();
    input2.insert(
        "todos".to_string(),
        serde_json::json!([
            {"content": "Old task", "status": "completed", "priority": "high"},
            {"content": "New task", "status": "in_progress", "priority": "medium"},
        ]),
    );
    let entries = vec![
        TranscriptEntry::ToolUse {
            timestamp: "2026-04-02T10:00:00Z".to_string(),
            tool_name: "todowrite".to_string(),
            tool_input: input1,
        },
        TranscriptEntry::ToolUse {
            timestamp: "2026-04-02T10:01:00Z".to_string(),
            tool_name: "bash".to_string(),
            tool_input: serde_json::Map::new(),
        },
        TranscriptEntry::ToolUse {
            timestamp: "2026-04-02T10:02:00Z".to_string(),
            tool_name: "todowrite".to_string(),
            tool_input: input2,
        },
    ];
    let todos = extract_todos_from_transcript(&entries);
    assert_eq!(todos.len(), 2);
    assert_eq!(todos[0].content, "Old task");
    assert_eq!(todos[0].status, TodoStatus::Completed);
    assert_eq!(todos[1].content, "New task");
    assert_eq!(todos[1].status, TodoStatus::InProgress);
}

#[test]
fn test_extract_todos_handles_missing_fields() {
    let mut input = serde_json::Map::new();
    input.insert(
        "todos".to_string(),
        serde_json::json!([
            {"content": "Has content only"},
            {"status": "pending", "priority": "high"},
        ]),
    );
    let entries = vec![TranscriptEntry::ToolUse {
        timestamp: "2026-04-02T10:00:00Z".to_string(),
        tool_name: "todowrite".to_string(),
        tool_input: input,
    }];
    let todos = extract_todos_from_transcript(&entries);
    // Items missing "content" are skipped; items missing status/priority get defaults
    assert_eq!(todos.len(), 1);
    assert_eq!(todos[0].content, "Has content only");
    assert_eq!(todos[0].status, TodoStatus::Pending); // default
    assert_eq!(todos[0].priority, TodoPriority::Medium); // default
}
