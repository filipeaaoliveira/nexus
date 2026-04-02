use ai_overview::detect::claude::*;
use ai_overview::types::PrimaryStatus;

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
