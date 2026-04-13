use crate::detect::claude::*;
use crate::detect::registry::ClaudeLaunchInfo;
use crate::domain::agents::{
    Activity, NormalizedReason, NormalizedStatus, PrimaryStatus, TodoPriority, TodoStatus,
};
use crate::domain::shared::from_rfc3339;

#[test]
fn test_infer_activity_tool_use_last() {
    let entries = vec![TranscriptEntry::ToolUse {
        timestamp: "2026-03-31T09:18:37.023Z".to_string(),
        tool_name: "read".to_string(),
        tool_input: serde_json::Map::new(),
    }];
    assert_eq!(
        infer_activity_from_last_entry(&entries),
        Some(Activity::ToolCall)
    );
}

#[test]
fn test_infer_activity_tool_result_last() {
    let content = include_str!("../../../tests/fixtures/claude_transcript_tool_result_last.jsonl");
    let entries = parse_transcript(content);
    assert_eq!(
        infer_activity_from_last_entry(&entries),
        Some(Activity::Thinking)
    );
}

#[test]
fn test_infer_activity_user_last() {
    let content = include_str!("../../../tests/fixtures/claude_transcript_user_last.jsonl");
    let entries = parse_transcript(content);
    assert_eq!(
        infer_activity_from_last_entry(&entries),
        Some(Activity::Generating)
    );
}

#[test]
fn test_infer_activity_empty() {
    assert_eq!(infer_activity_from_last_entry(&[]), None);
}

#[test]
fn test_parse_transcript_events() {
    let content = include_str!("../../../tests/fixtures/claude_transcript.jsonl");
    let events = parse_transcript(content);
    assert_eq!(events.len(), 9);
}

#[test]
fn test_extract_tool_calls() {
    let content = include_str!("../../../tests/fixtures/claude_transcript.jsonl");
    let events = parse_transcript(content);
    let tool_calls: Vec<_> = events
        .iter()
        .filter(|e| matches!(e, TranscriptEntry::ToolUse { .. }))
        .collect();
    assert_eq!(tool_calls.len(), 4); // glob, read, edit, task
}

#[test]
fn test_detect_subagent_spawn() {
    let content = include_str!("../../../tests/fixtures/claude_transcript.jsonl");
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
    let content = include_str!("../../../tests/fixtures/claude_transcript.jsonl");
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
fn test_normalize_claude_review_text_maps_to_needs_input_review() {
    let entries = vec![TranscriptEntry::Assistant {
        timestamp: "2026-04-02T10:00:00Z".to_string(),
        content: "### Issues\n#### Critical\n- The implementation is missing error handling."
            .to_string(),
    }];

    let normalized = normalize_claude_state(&entries, false);

    assert_eq!(normalized.status, NormalizedStatus::NeedsInput);
    assert_eq!(normalized.reason, Some(NormalizedReason::Review));
}

#[test]
fn test_normalize_claude_generic_issues_analysis_does_not_map_to_review() {
    let entries = vec![TranscriptEntry::Assistant {
        timestamp: "2026-04-02T10:00:00Z".to_string(),
        content: "Issues:\nI reviewed the codebase and wrote down observations.".to_string(),
    }];

    let normalized = normalize_claude_state(&entries, false);

    assert_eq!(normalized.status, NormalizedStatus::Working);
    assert_eq!(normalized.reason, None);
}

#[test]
fn test_parse_transcript_parses_assistant_entry_and_normalizes_working() {
    let content = concat!(
        "{\"type\":\"assistant\",\"timestamp\":\"2026-04-02T10:00:00Z\",\"content\":[",
        "{\"type\":\"text\",\"text\":\"I have reviewed the files and I am moving on.\"}",
        "]}\n"
    );

    let entries = parse_transcript(content);
    assert_eq!(entries.len(), 1);
    assert!(matches!(entries[0], TranscriptEntry::Assistant { .. }));

    let normalized = normalize_claude_state(&entries, false);
    assert_eq!(normalized.status, NormalizedStatus::Working);
    assert_eq!(normalized.reason, None);
}

#[test]
fn test_normalize_claude_recent_generic_assistant_text_maps_to_working() {
    let entries = vec![TranscriptEntry::Assistant {
        timestamp: "2026-04-02T10:00:00Z".to_string(),
        content: "I have reviewed the files and I am moving on.".to_string(),
    }];

    let normalized = normalize_claude_state(&entries, false);

    assert_eq!(normalized.status, NormalizedStatus::Working);
    assert_eq!(normalized.reason, None);
}

#[test]
fn test_normalize_claude_stale_trailing_user_maps_to_done() {
    let entries = vec![TranscriptEntry::User {
        timestamp: "2026-04-02T10:00:00Z".to_string(),
        content: "Do something".to_string(),
    }];

    let normalized = normalize_claude_state(&entries, true);

    // A stale trailing user message means Claude received input but produced
    // no output for 30+ seconds — the session is idle, not working.
    assert_eq!(normalized.status, NormalizedStatus::Done);
    assert_eq!(normalized.reason, Some(NormalizedReason::ReadyForNextTask));
}

#[test]
fn test_normalize_claude_stale_review_still_maps_to_needs_input_review() {
    let entries = vec![TranscriptEntry::Assistant {
        timestamp: "2026-04-02T10:00:00Z".to_string(),
        content: "### Issues\n#### Critical\n- The implementation is missing error handling."
            .to_string(),
    }];

    let normalized = normalize_claude_state(&entries, true);

    assert_eq!(normalized.status, NormalizedStatus::NeedsInput);
    assert_eq!(normalized.reason, Some(NormalizedReason::Review));
}

#[test]
fn test_normalize_claude_stale_done_still_maps_to_ready_for_next_task() {
    let entries = vec![TranscriptEntry::Assistant {
        timestamp: "2026-04-02T10:00:00Z".to_string(),
        content: "The changes are complete and ready for the next task.".to_string(),
    }];

    let normalized = normalize_claude_state(&entries, true);

    assert_eq!(normalized.status, NormalizedStatus::Done);
    assert_eq!(normalized.reason, Some(NormalizedReason::ReadyForNextTask));
}

#[test]
fn test_normalize_claude_stale_blocked_still_maps_to_error() {
    let entries = vec![TranscriptEntry::Assistant {
        timestamp: "2026-04-02T10:00:00Z".to_string(),
        content: "Fatal error: cannot continue.".to_string(),
    }];

    let normalized = normalize_claude_state(&entries, true);

    assert_eq!(normalized.status, NormalizedStatus::Blocked);
    assert_eq!(normalized.reason, Some(NormalizedReason::Error));
}

#[test]
fn test_normalize_claude_generic_report_text_does_not_map_to_review() {
    let entries = vec![TranscriptEntry::Assistant {
        timestamp: "2026-04-02T10:00:00Z".to_string(),
        content: "Report:\nI investigated the codebase and summarized the findings.".to_string(),
    }];

    let normalized = normalize_claude_state(&entries, false);

    assert_eq!(normalized.status, NormalizedStatus::Working);
    assert_eq!(normalized.reason, None);
}

#[test]
fn test_normalize_claude_error_log_reference_does_not_map_to_blocked() {
    let entries = vec![TranscriptEntry::Assistant {
        timestamp: "2026-04-02T10:00:00Z".to_string(),
        content: "I found fatal error in the logs and fixed it.".to_string(),
    }];

    let normalized = normalize_claude_state(&entries, false);

    assert_eq!(normalized.status, NormalizedStatus::Working);
    assert_eq!(normalized.reason, None);
}

#[test]
fn test_normalize_claude_retrospective_could_not_complete_does_not_map_to_blocked() {
    let entries = vec![TranscriptEntry::Assistant {
        timestamp: "2026-04-02T10:00:00Z".to_string(),
        content: "The previous run could not complete, but I fixed it.".to_string(),
    }];

    let normalized = normalize_claude_state(&entries, false);

    assert_eq!(normalized.status, NormalizedStatus::Working);
    assert_eq!(normalized.reason, None);
}

#[test]
fn test_normalize_claude_retrospective_unable_to_continue_does_not_map_to_blocked() {
    let entries = vec![TranscriptEntry::Assistant {
        timestamp: "2026-04-02T10:00:00Z".to_string(),
        content: "The previous attempt was unable to continue, but the fix is in place now."
            .to_string(),
    }];

    let normalized = normalize_claude_state(&entries, false);

    assert_eq!(normalized.status, NormalizedStatus::Working);
    assert_eq!(normalized.reason, None);
}

#[test]
fn test_normalize_claude_stale_generic_assistant_text_stays_working() {
    let entries = vec![TranscriptEntry::Assistant {
        timestamp: "2026-04-02T10:00:00Z".to_string(),
        content: "I have reviewed the files and I am moving on.".to_string(),
    }];

    let normalized = normalize_claude_state(&entries, true);

    assert_eq!(normalized.status, NormalizedStatus::Working);
    assert_eq!(normalized.reason, None);
}

#[test]
fn test_normalize_claude_active_tool_use_maps_to_working_tool_call() {
    let entries = vec![
        TranscriptEntry::User {
            timestamp: "2026-04-02T10:00:00Z".to_string(),
            content: "Please inspect the route".to_string(),
        },
        TranscriptEntry::ToolUse {
            timestamp: "2026-04-02T10:00:01Z".to_string(),
            tool_name: "read".to_string(),
            tool_input: serde_json::Map::new(),
        },
    ];

    let normalized = normalize_claude_state(&entries, false);

    assert_eq!(normalized.status, NormalizedStatus::Working);
    assert_eq!(normalized.reason, Some(NormalizedReason::ToolCall));
}

#[test]
fn test_normalize_claude_agent_tool_use_maps_to_working_subagent() {
    let entries = vec![
        TranscriptEntry::User {
            timestamp: "2026-04-02T10:00:00Z".to_string(),
            content: "Please delegate this task".to_string(),
        },
        TranscriptEntry::ToolUse {
            timestamp: "2026-04-02T10:00:01Z".to_string(),
            tool_name: "Agent".to_string(),
            tool_input: serde_json::Map::new(),
        },
    ];

    let normalized = normalize_claude_state(&entries, false);

    assert_eq!(normalized.status, NormalizedStatus::Working);
    assert_eq!(normalized.reason, Some(NormalizedReason::Subagent));
}

#[test]
fn test_normalize_claude_completed_work_maps_to_done_ready_for_next_task() {
    let entries = vec![TranscriptEntry::Assistant {
        timestamp: "2026-04-02T10:00:00Z".to_string(),
        content: "The changes are complete and ready for the next task.".to_string(),
    }];

    let normalized = normalize_claude_state(&entries, false);

    assert_eq!(normalized.status, NormalizedStatus::Done);
    assert_eq!(normalized.reason, Some(NormalizedReason::ReadyForNextTask));
}

#[test]
fn test_normalize_claude_requested_changes_completion_stays_done() {
    let entries = vec![TranscriptEntry::Assistant {
        timestamp: "2026-04-02T10:00:00Z".to_string(),
        content: "Implemented the requested changes and ready for the next task.".to_string(),
    }];

    let normalized = normalize_claude_state(&entries, false);

    assert_eq!(normalized.status, NormalizedStatus::Done);
    assert_eq!(normalized.reason, Some(NormalizedReason::ReadyForNextTask));
}

#[test]
fn test_normalize_claude_newer_user_overrides_older_assistant_review() {
    let entries = vec![
        TranscriptEntry::Assistant {
            timestamp: "2026-04-02T10:00:00Z".to_string(),
            content: "### Issues\n#### Critical\n- The implementation is missing error handling."
                .to_string(),
        },
        TranscriptEntry::User {
            timestamp: "2026-04-02T10:01:00Z".to_string(),
            content: "Please keep going.".to_string(),
        },
    ];

    let normalized = normalize_claude_state(&entries, false);

    assert_eq!(normalized.status, NormalizedStatus::Working);
    assert_eq!(normalized.reason, None);
}

#[test]
fn test_normalize_claude_newer_tool_result_overrides_older_assistant_review() {
    let entries = vec![
        TranscriptEntry::Assistant {
            timestamp: "2026-04-02T10:00:00Z".to_string(),
            content: "### Issues\n#### Critical\n- The implementation is missing error handling."
                .to_string(),
        },
        TranscriptEntry::ToolResult {
            timestamp: "2026-04-02T10:01:00Z".to_string(),
            tool_name: "read".to_string(),
        },
    ];

    let normalized = normalize_claude_state(&entries, false);

    assert_eq!(normalized.status, NormalizedStatus::Working);
    assert_eq!(normalized.reason, Some(NormalizedReason::ToolCall));
}

#[test]
fn test_normalize_claude_stale_empty_entries_map_to_unknown() {
    let entries: Vec<TranscriptEntry> = vec![];

    let normalized = normalize_claude_state(&entries, true);

    assert_eq!(normalized.status, NormalizedStatus::Unknown);
    assert_eq!(normalized.reason, None);
}

#[test]
fn test_normalize_claude_stale_trailing_tool_result_stays_working() {
    let entries = vec![
        TranscriptEntry::ToolUse {
            timestamp: "2026-04-02T10:00:00Z".to_string(),
            tool_name: "read".to_string(),
            tool_input: serde_json::Map::new(),
        },
        TranscriptEntry::ToolResult {
            timestamp: "2026-04-02T10:00:01Z".to_string(),
            tool_name: "read".to_string(),
        },
    ];

    let normalized = normalize_claude_state(&entries, true);

    // A stale tool result means Claude was still mid-turn when transcript
    // writes stopped. Keep the session in Working rather than declaring it
    // ready for the next task without an explicit completion signal.
    assert_eq!(normalized.status, NormalizedStatus::Working);
    assert_eq!(normalized.reason, Some(NormalizedReason::ToolCall));
}

#[test]
fn test_normalize_claude_stale_trailing_tool_use_stays_working() {
    let entries = vec![TranscriptEntry::ToolUse {
        timestamp: "2026-04-02T10:00:00Z".to_string(),
        tool_name: "read".to_string(),
        tool_input: serde_json::Map::new(),
    }];

    let normalized = normalize_claude_state(&entries, true);

    assert_eq!(normalized.status, NormalizedStatus::Working);
    assert_eq!(normalized.reason, Some(NormalizedReason::ToolCall));
}

#[test]
fn test_normalize_claude_stale_trailing_user_continuation_maps_to_done() {
    let entries = vec![TranscriptEntry::User {
        timestamp: "2026-04-02T10:00:00Z".to_string(),
        content: "Please keep going.".to_string(),
    }];

    let normalized = normalize_claude_state(&entries, true);

    assert_eq!(normalized.status, NormalizedStatus::Done);
    assert_eq!(normalized.reason, Some(NormalizedReason::ReadyForNextTask));
}

#[test]
fn test_normalize_claude_stale_generic_assistant_after_older_review_stays_working() {
    let entries = vec![
        TranscriptEntry::Assistant {
            timestamp: "2026-04-02T10:00:00Z".to_string(),
            content: "### Issues\n#### Critical\n- The implementation is missing error handling."
                .to_string(),
        },
        TranscriptEntry::Assistant {
            timestamp: "2026-04-02T10:01:00Z".to_string(),
            content: "I have reviewed the files and I am moving on.".to_string(),
        },
    ];

    let normalized = normalize_claude_state(&entries, true);

    assert_eq!(normalized.status, NormalizedStatus::Working);
    assert_eq!(normalized.reason, None);
}

#[test]
fn test_parse_project_session_extracts_branch() {
    let content = include_str!("../../../tests/fixtures/claude_project_session.jsonl");
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
    let content = include_str!("../../../tests/fixtures/claude_project_session.jsonl");
    let info = parse_project_session(content);
    assert!(info.has_internal_initiator);
}

/// Ensures that processing a transcript ending with tool_result does NOT
/// emit a ToolCallCompleted event (which would cause activity=Idle).
/// We verify this by checking that the last entry type after a tool_result
/// maps to Thinking, not to something that would produce ToolCallCompleted.
#[test]
fn test_tool_result_does_not_imply_idle_activity() {
    let content = include_str!("../../../tests/fixtures/claude_transcript_tool_result_last.jsonl");
    let entries = parse_transcript(content);
    // The activity inferred should be Thinking, never Idle
    let activity = infer_activity_from_last_entry(&entries);
    assert_ne!(activity, Some(Activity::Idle));
    assert_eq!(activity, Some(Activity::Thinking));
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
        PrimaryStatus::Waiting
    );
}

#[test]
fn test_is_stale_old_mtime() {
    use std::time::{Duration, SystemTime};
    let old = SystemTime::now() - Duration::from_secs(30);
    assert!(crate::detect::claude::is_stale(old, 10));
}

#[test]
fn test_is_stale_recent_mtime() {
    use std::time::SystemTime;
    let now = SystemTime::now();
    assert!(!crate::detect::claude::is_stale(now, 10));
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

/// After a compact_boundary event the agent emits a synthetic user message to
/// restore session context.  That restore message should still report
/// Activity::Compacting — not Generating — so the UI shows "compacting" until
/// the agent actually starts generating again.
#[test]
fn test_infer_activity_restore_message_after_compaction_is_compacting() {
    let entries = vec![
        TranscriptEntry::CompactBoundary {
            timestamp: "2026-04-02T10:00:00Z".to_string(),
        },
        TranscriptEntry::User {
            timestamp: "2026-04-02T10:00:01Z".to_string(),
            content: "[restore checkpointed session agent configuration after compaction]"
                .to_string(),
        },
    ];
    let activity = infer_activity_from_last_entry(&entries);
    assert_eq!(activity, Some(Activity::Compacting));
}

#[test]
fn test_transcript_start_index_for_process_skips_old_entries_for_fresh_launch() {
    let started_at = from_rfc3339("2026-04-02T10:00:30Z").unwrap();
    let entries = vec![
        TranscriptEntry::ToolUse {
            timestamp: "2026-04-02T10:00:00Z".to_string(),
            tool_name: "todowrite".to_string(),
            tool_input: serde_json::json!({
                "todos": [
                    {"content": "Stale todo", "status": "pending", "priority": "medium"}
                ]
            })
            .as_object()
            .cloned()
            .unwrap(),
        },
        TranscriptEntry::User {
            timestamp: "2026-04-02T10:00:10Z".to_string(),
            content: "Old prompt".to_string(),
        },
        TranscriptEntry::ToolUse {
            timestamp: "2026-04-02T10:00:40Z".to_string(),
            tool_name: "todowrite".to_string(),
            tool_input: serde_json::json!({
                "todos": [
                    {"content": "Live todo", "status": "in_progress", "priority": "high"}
                ]
            })
            .as_object()
            .cloned()
            .unwrap(),
        },
        TranscriptEntry::User {
            timestamp: "2026-04-02T10:01:00Z".to_string(),
            content: "Keep going".to_string(),
        },
    ];

    let start_index = transcript_start_index_for_process(&entries, Some(started_at), false);
    assert_eq!(start_index, 2);

    let todos = extract_todos_from_transcript(&entries[start_index..]);
    assert_eq!(todos.len(), 1);
    assert_eq!(todos[0].content, "Live todo");
    assert_eq!(todos[0].status, TodoStatus::InProgress);
}

#[test]
fn test_transcript_start_index_for_process_does_not_skip_old_entries_for_resume_launch() {
    let started_at = from_rfc3339("2026-04-02T10:00:30Z").unwrap();
    let entries = vec![
        TranscriptEntry::ToolUse {
            timestamp: "2026-04-02T10:00:00Z".to_string(),
            tool_name: "todowrite".to_string(),
            tool_input: serde_json::json!({
                "todos": [
                    {"content": "Resumed todo", "status": "pending", "priority": "medium"}
                ]
            })
            .as_object()
            .cloned()
            .unwrap(),
        },
        TranscriptEntry::User {
            timestamp: "2026-04-02T10:00:10Z".to_string(),
            content: "Old prompt".to_string(),
        },
    ];

    let start_index = transcript_start_index_for_process(&entries, Some(started_at), true);
    assert_eq!(start_index, 0);

    let todos = extract_todos_from_transcript(&entries[start_index..]);
    assert_eq!(todos.len(), 1);
    assert_eq!(todos[0].content, "Resumed todo");
}

#[test]
fn test_should_hydrate_session_history_requires_matching_resume_session_id() {
    let fresh = ClaudeLaunchInfo {
        started_at: Some(from_rfc3339("2026-04-02T10:00:00Z").unwrap()),
        resume_session_id: None,
    };
    let resumed = ClaudeLaunchInfo {
        started_at: Some(from_rfc3339("2026-04-02T10:00:00Z").unwrap()),
        resume_session_id: Some("session-123".to_string()),
    };
    let matching_session = Some(&crate::detect::claude::ProjectSessionInfo {
        session_id: Some("session-123".to_string()),
        branch: None,
        cwd: None,
        model: None,
        has_internal_initiator: false,
        started_at: None,
    });
    let mismatched_session = Some(&crate::detect::claude::ProjectSessionInfo {
        session_id: Some("session-999".to_string()),
        branch: None,
        cwd: None,
        model: None,
        has_internal_initiator: false,
        started_at: None,
    });

    assert!(!should_hydrate_session_history(
        Some(&fresh),
        matching_session
    ));
    assert!(should_hydrate_session_history(
        Some(&resumed),
        matching_session
    ));
    assert!(!should_hydrate_session_history(
        Some(&resumed),
        mismatched_session
    ));
    assert!(!should_hydrate_session_history(None, matching_session));
}

#[test]
fn test_is_resume_launch_comes_from_process_metadata_only() {
    let resume_launch = ClaudeLaunchInfo {
        started_at: Some(from_rfc3339("2026-04-02T10:00:00Z").unwrap()),
        resume_session_id: Some("session-123".to_string()),
    };
    let fresh_launch = ClaudeLaunchInfo {
        started_at: Some(from_rfc3339("2026-04-02T10:00:00Z").unwrap()),
        resume_session_id: None,
    };

    assert!(is_resume_launch(Some(&resume_launch)));
    assert!(!is_resume_launch(Some(&fresh_launch)));
    assert!(!is_resume_launch(None));
}

#[test]
fn test_session_info_sent_is_retryable_for_resume_until_historical_session_is_emitted() {
    assert!(should_mark_session_info_sent(false, false));
    assert!(!should_mark_session_info_sent(true, false));
    assert!(should_mark_session_info_sent(true, true));
}

#[test]
fn test_activity_uses_retained_slice_after_fresh_launch_cutoff() {
    let retained_entries = vec![
        TranscriptEntry::User {
            timestamp: "2026-04-02T10:00:00Z".to_string(),
            content: "Please inspect the files".to_string(),
        },
        TranscriptEntry::ToolUse {
            timestamp: "2026-04-02T10:00:01Z".to_string(),
            tool_name: "read".to_string(),
            tool_input: serde_json::Map::new(),
        },
        TranscriptEntry::ToolResult {
            timestamp: "2026-04-02T10:00:02Z".to_string(),
            tool_name: "read".to_string(),
        },
        TranscriptEntry::Assistant {
            timestamp: "2026-04-02T10:00:03Z".to_string(),
            content: "I have the file contents.".to_string(),
        },
    ];

    assert_eq!(
        infer_activity_from_last_entry(&retained_entries),
        Some(Activity::Thinking)
    );
}

#[test]
fn test_extract_todos_from_fresh_launch_keeps_last_todowrite_after_non_todowrite_append() {
    let entries = vec![
        TranscriptEntry::ToolUse {
            timestamp: "2026-04-02T10:00:00Z".to_string(),
            tool_name: "todowrite".to_string(),
            tool_input: serde_json::json!({
                "todos": [
                    {"content": "Initial todo", "status": "pending", "priority": "medium"}
                ]
            })
            .as_object()
            .cloned()
            .unwrap(),
        },
        TranscriptEntry::User {
            timestamp: "2026-04-02T10:01:00Z".to_string(),
            content: "Just a progress note".to_string(),
        },
        TranscriptEntry::ToolUse {
            timestamp: "2026-04-02T10:02:00Z".to_string(),
            tool_name: "read".to_string(),
            tool_input: serde_json::Map::new(),
        },
    ];

    let todos = extract_todos_from_transcript(&entries);
    assert_eq!(todos.len(), 1);
    assert_eq!(todos[0].content, "Initial todo");
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

// ── Nested project-file format ──────────────────────────────────────────────

#[test]
fn test_parse_project_file_nested_format_extracts_tool_use_from_assistant() {
    let content = include_str!("../../../tests/fixtures/claude_project_session_nested.jsonl");
    let entries = parse_transcript(content);

    // Expect: User, ToolUse(Read), ToolUse(Bash), Assistant(""), ToolResult×2, Assistant(text)
    let tool_uses: Vec<_> = entries
        .iter()
        .filter_map(|e| match e {
            TranscriptEntry::ToolUse { tool_name, .. } => Some(tool_name.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(
        tool_uses,
        vec!["Read", "Bash"],
        "should extract tool_use blocks from assistant message.content"
    );
}

#[test]
fn test_parse_project_file_nested_format_extracts_tool_result_from_user() {
    let content = include_str!("../../../tests/fixtures/claude_project_session_nested.jsonl");
    let entries = parse_transcript(content);

    let tool_results: Vec<_> = entries
        .iter()
        .filter(|e| matches!(e, TranscriptEntry::ToolResult { .. }))
        .collect();
    assert_eq!(
        tool_results.len(),
        2,
        "should extract tool_result blocks from user message.content"
    );
}

#[test]
fn test_parse_project_file_nested_format_normalizes_to_done() {
    // The fixture ends with an assistant message containing "Ready for the next task."
    let content = include_str!("../../../tests/fixtures/claude_project_session_nested.jsonl");
    let entries = parse_transcript(content);

    let state = normalize_claude_state(&entries, false);
    assert_eq!(
        state.status,
        NormalizedStatus::Done,
        "should classify final assistant text as Done"
    );
}

#[test]
fn test_parse_project_file_nested_format_thinking_blocks_not_emitted() {
    // "thinking" content blocks must not be emitted as Assistant entries
    // (they are internal monologue and should be invisible to state inference).
    let content = include_str!("../../../tests/fixtures/claude_project_session_nested.jsonl");
    let entries = parse_transcript(content);

    let assistant_entries: Vec<_> = entries
        .iter()
        .filter_map(|e| match e {
            TranscriptEntry::Assistant { content, .. } => Some(content.as_str()),
            _ => None,
        })
        .collect();
    // None of the assistant entries should contain the raw thinking text.
    for content in &assistant_entries {
        assert!(
            !content.contains("The README says"),
            "thinking block text must not appear in Assistant entries"
        );
    }
}

// ── system entry parsing tests ─────────────────────────────────────

#[test]
fn test_parse_transcript_system_turn_duration_emits_turn_completed() {
    let content = concat!(
        "{\"type\":\"user\",\"timestamp\":\"2026-04-09T10:00:00Z\",\"content\":\"hello\"}\n",
        "{\"type\":\"assistant\",\"timestamp\":\"2026-04-09T10:00:05Z\",\"content\":[{\"type\":\"text\",\"text\":\"Done.\"}]}\n",
        "{\"type\":\"system\",\"subtype\":\"turn_duration\",\"timestamp\":\"2026-04-09T10:00:06Z\",\"durationMs\":6000}\n",
    );
    let entries = parse_transcript(content);
    assert!(
        matches!(entries.last(), Some(TranscriptEntry::TurnCompleted { .. })),
        "system:turn_duration should produce TurnCompleted entry"
    );
}

#[test]
fn test_parse_transcript_system_api_error_emits_api_error() {
    let content = concat!(
        "{\"type\":\"user\",\"timestamp\":\"2026-04-09T10:00:00Z\",\"content\":\"hello\"}\n",
        "{\"type\":\"system\",\"subtype\":\"api_error\",\"timestamp\":\"2026-04-09T10:00:02Z\",",
        "\"error\":\"overloaded\",\"retryInMs\":5000,\"retryAttempt\":1,\"maxRetries\":3}\n",
    );
    let entries = parse_transcript(content);
    assert!(
        matches!(
            entries.last(),
            Some(TranscriptEntry::ApiError {
                retry_attempt: Some(1),
                max_retries: Some(3),
                ..
            })
        ),
        "system:api_error should produce ApiError entry"
    );
}

#[test]
fn test_parse_transcript_ignores_unknown_system_subtypes() {
    let content = "{\"type\":\"system\",\"subtype\":\"local_command\",\"timestamp\":\"2026-04-09T10:00:00Z\",\"content\":\"ls\"}\n";
    let entries = parse_transcript(content);
    assert!(entries.is_empty(), "local_command should be ignored");
}

#[test]
fn test_parse_transcript_compact_boundary_emits_entry() {
    let content = "{\"type\":\"system\",\"subtype\":\"compact_boundary\",\"timestamp\":\"2026-04-09T10:00:01Z\"}\n";
    let entries = parse_transcript(content);
    assert_eq!(
        entries.len(),
        1,
        "compact_boundary should produce one entry"
    );
    assert!(
        matches!(entries[0], TranscriptEntry::CompactBoundary { .. }),
        "compact_boundary should produce a CompactBoundary entry"
    );
}

// ── normalize: TurnCompleted ──────────────────────────────────────

#[test]
fn test_normalize_turn_completed_maps_to_done() {
    let entries = vec![
        TranscriptEntry::Assistant {
            timestamp: "2026-04-09T10:00:05Z".to_string(),
            content: "I've made the changes you requested.".to_string(),
        },
        TranscriptEntry::TurnCompleted {
            timestamp: "2026-04-09T10:00:06Z".to_string(),
        },
    ];

    let normalized = normalize_claude_state(&entries, false);
    assert_eq!(normalized.status, NormalizedStatus::Done);
    assert_eq!(normalized.reason, Some(NormalizedReason::ReadyForNextTask));
}

#[test]
fn test_normalize_stale_turn_completed_maps_to_done() {
    let entries = vec![
        TranscriptEntry::Assistant {
            timestamp: "2026-04-09T10:00:05Z".to_string(),
            content: "I've made the changes you requested.".to_string(),
        },
        TranscriptEntry::TurnCompleted {
            timestamp: "2026-04-09T10:00:06Z".to_string(),
        },
    ];

    let normalized = normalize_claude_state(&entries, true);
    assert_eq!(normalized.status, NormalizedStatus::Done);
    assert_eq!(normalized.reason, Some(NormalizedReason::ReadyForNextTask));
}

#[test]
fn test_normalize_turn_completed_after_user_message_maps_to_done() {
    // User sends a follow-up, assistant responds, turn completes
    let entries = vec![
        TranscriptEntry::User {
            timestamp: "2026-04-09T10:00:00Z".to_string(),
            content: "Do something".to_string(),
        },
        TranscriptEntry::Assistant {
            timestamp: "2026-04-09T10:00:05Z".to_string(),
            content: "Done.".to_string(),
        },
        TranscriptEntry::TurnCompleted {
            timestamp: "2026-04-09T10:00:06Z".to_string(),
        },
    ];

    let normalized = normalize_claude_state(&entries, false);
    assert_eq!(normalized.status, NormalizedStatus::Done);
    assert_eq!(normalized.reason, Some(NormalizedReason::ReadyForNextTask));
}

// ── normalize: ApiError ───────────────────────────────────────────

#[test]
fn test_normalize_api_error_maps_to_working_retrying() {
    let entries = vec![
        TranscriptEntry::User {
            timestamp: "2026-04-09T10:00:00Z".to_string(),
            content: "Do something".to_string(),
        },
        TranscriptEntry::ApiError {
            timestamp: "2026-04-09T10:00:02Z".to_string(),
            retry_attempt: Some(1),
            max_retries: Some(3),
        },
    ];

    let normalized = normalize_claude_state(&entries, false);
    assert_eq!(normalized.status, NormalizedStatus::Working);
    assert_eq!(normalized.reason, Some(NormalizedReason::Retrying));
}

#[test]
fn test_normalize_stale_api_error_maps_to_working_retrying() {
    let entries = vec![
        TranscriptEntry::User {
            timestamp: "2026-04-09T10:00:00Z".to_string(),
            content: "Do something".to_string(),
        },
        TranscriptEntry::ApiError {
            timestamp: "2026-04-09T10:00:02Z".to_string(),
            retry_attempt: Some(2),
            max_retries: Some(3),
        },
    ];

    let normalized = normalize_claude_state(&entries, true);
    assert_eq!(normalized.status, NormalizedStatus::Working);
    assert_eq!(normalized.reason, Some(NormalizedReason::Retrying));
}

// ── normalize: stale ToolUse remains working without explicit approval ─────

#[test]
fn test_normalize_stale_agent_tool_use_stays_working_subagent() {
    let entries = vec![TranscriptEntry::ToolUse {
        timestamp: "2026-04-09T10:00:00Z".to_string(),
        tool_name: "Agent".to_string(),
        tool_input: serde_json::Map::new(),
    }];

    let normalized = normalize_claude_state(&entries, true);
    assert_eq!(normalized.status, NormalizedStatus::Working);
    assert_eq!(normalized.reason, Some(NormalizedReason::Subagent));
}

// ── infer_activity: new variants are transparent ─────────────────

#[test]
fn test_infer_activity_skips_turn_completed() {
    // TurnCompleted should be transparent to activity inference — it should
    // look through to the user entry underneath.
    let entries = vec![
        TranscriptEntry::User {
            timestamp: "2026-04-09T10:00:00Z".to_string(),
            content: "hello".to_string(),
        },
        TranscriptEntry::TurnCompleted {
            timestamp: "2026-04-09T10:00:06Z".to_string(),
        },
    ];

    // last_non_assistant_entry skips TurnCompleted, so we see User → Generating
    assert_eq!(
        infer_activity_from_last_entry(&entries),
        Some(Activity::Generating)
    );
}

#[test]
fn test_infer_activity_skips_api_error() {
    let entries = vec![
        TranscriptEntry::ToolUse {
            timestamp: "2026-04-09T10:00:00Z".to_string(),
            tool_name: "read".to_string(),
            tool_input: serde_json::Map::new(),
        },
        TranscriptEntry::ApiError {
            timestamp: "2026-04-09T10:00:02Z".to_string(),
            retry_attempt: Some(1),
            max_retries: Some(3),
        },
    ];

    // last_non_assistant_entry skips ApiError, so we see ToolUse → ToolCall
    assert_eq!(
        infer_activity_from_last_entry(&entries),
        Some(Activity::ToolCall)
    );
}

// ── CompactBoundary ───────────────────────────────────────────────

#[test]
fn test_infer_activity_compact_boundary_entry() {
    let entries = vec![TranscriptEntry::CompactBoundary {
        timestamp: "2026-04-09T10:00:00Z".to_string(),
    }];
    assert_eq!(
        infer_activity_from_last_entry(&entries),
        Some(Activity::Compacting)
    );
}

#[test]
fn test_normalize_compact_boundary_maps_to_working_compacting() {
    let entries = vec![TranscriptEntry::CompactBoundary {
        timestamp: "2026-04-09T10:00:00Z".to_string(),
    }];
    let normalized = normalize_claude_state(&entries, false);
    assert_eq!(normalized.status, NormalizedStatus::Working);
    assert_eq!(normalized.reason, Some(NormalizedReason::Compacting));
}

#[test]
fn test_normalize_stale_compact_boundary_maps_to_working_compacting() {
    let entries = vec![TranscriptEntry::CompactBoundary {
        timestamp: "2026-04-09T10:00:00Z".to_string(),
    }];
    let normalized = normalize_claude_state(&entries, true);
    assert_eq!(normalized.status, NormalizedStatus::Working);
    assert_eq!(normalized.reason, Some(NormalizedReason::Compacting));
}

// ── TurnBlocked (stop_reason: max_tokens / refusal / pause_turn) ──

#[test]
fn test_parse_transcript_stop_reason_max_tokens_emits_turn_blocked() {
    let content = concat!(
        "{\"type\":\"assistant\",\"timestamp\":\"2026-04-09T10:00:00Z\",",
        "\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"I was in the middle\"}],",
        "\"stop_reason\":\"max_tokens\"}}\n"
    );
    let entries = parse_transcript(content);
    assert!(
        entries.iter().any(|e| matches!(
            e,
            TranscriptEntry::TurnBlocked { stop_reason, .. } if stop_reason == "max_tokens"
        )),
        "max_tokens stop_reason should produce a TurnBlocked entry"
    );
}

#[test]
fn test_parse_transcript_stop_reason_refusal_emits_turn_blocked() {
    let content = concat!(
        "{\"type\":\"assistant\",\"timestamp\":\"2026-04-09T10:00:00Z\",",
        "\"message\":{\"role\":\"assistant\",\"content\":[],",
        "\"stop_reason\":\"refusal\"}}\n"
    );
    let entries = parse_transcript(content);
    assert!(
        entries.iter().any(|e| matches!(
            e,
            TranscriptEntry::TurnBlocked { stop_reason, .. } if stop_reason == "refusal"
        )),
        "refusal stop_reason should produce a TurnBlocked entry"
    );
}

#[test]
fn test_parse_transcript_stop_reason_pause_turn_emits_turn_blocked() {
    let content = concat!(
        "{\"type\":\"assistant\",\"timestamp\":\"2026-04-09T10:00:00Z\",",
        "\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"Paused.\"}],",
        "\"stop_reason\":\"pause_turn\"}}\n"
    );
    let entries = parse_transcript(content);
    assert!(
        entries.iter().any(|e| matches!(
            e,
            TranscriptEntry::TurnBlocked { stop_reason, .. } if stop_reason == "pause_turn"
        )),
        "pause_turn stop_reason should produce a TurnBlocked entry"
    );
}

#[test]
fn test_parse_transcript_stop_reason_end_turn_does_not_emit_turn_blocked() {
    let content = concat!(
        "{\"type\":\"assistant\",\"timestamp\":\"2026-04-09T10:00:00Z\",",
        "\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"Done.\"}],",
        "\"stop_reason\":\"end_turn\"}}\n"
    );
    let entries = parse_transcript(content);
    assert!(
        !entries
            .iter()
            .any(|e| matches!(e, TranscriptEntry::TurnBlocked { .. })),
        "end_turn should NOT produce a TurnBlocked entry"
    );
}

#[test]
fn test_normalize_turn_blocked_maps_to_blocked_error() {
    let entries = vec![
        TranscriptEntry::Assistant {
            timestamp: "2026-04-09T10:00:00Z".to_string(),
            content: "I was mid-sentence when".to_string(),
        },
        TranscriptEntry::TurnBlocked {
            timestamp: "2026-04-09T10:00:00Z".to_string(),
            stop_reason: "max_tokens".to_string(),
        },
    ];
    let normalized = normalize_claude_state(&entries, false);
    assert_eq!(normalized.status, NormalizedStatus::Blocked);
    assert_eq!(normalized.reason, Some(NormalizedReason::Error));
}

#[test]
fn test_normalize_stale_turn_blocked_maps_to_blocked_error() {
    let entries = vec![TranscriptEntry::TurnBlocked {
        timestamp: "2026-04-09T10:00:00Z".to_string(),
        stop_reason: "refusal".to_string(),
    }];
    let normalized = normalize_claude_state(&entries, true);
    assert_eq!(normalized.status, NormalizedStatus::Blocked);
    assert_eq!(normalized.reason, Some(NormalizedReason::Error));
}

#[test]
fn test_infer_activity_turn_blocked_returns_none() {
    let entries = vec![TranscriptEntry::TurnBlocked {
        timestamp: "2026-04-09T10:00:00Z".to_string(),
        stop_reason: "max_tokens".to_string(),
    }];
    assert_eq!(infer_activity_from_last_entry(&entries), None);
}

// ── LocalCommand entries ──────────────────────────────────────────

#[test]
fn test_parse_transcript_local_command_caveat_emits_local_command() {
    let content = concat!(
        "{\"type\":\"user\",\"timestamp\":\"2026-04-09T10:00:00Z\",",
        "\"content\":\"<local-command-caveat>Caveat: The messages below were generated by the user while running local commands.</local-command-caveat>\"}\n"
    );
    let entries = parse_transcript(content);
    assert_eq!(entries.len(), 1);
    assert!(
        matches!(entries[0], TranscriptEntry::LocalCommand { .. }),
        "<local-command-caveat> content should emit LocalCommand"
    );
}

#[test]
fn test_parse_transcript_local_command_stdout_emits_local_command() {
    let content = concat!(
        "{\"type\":\"user\",\"timestamp\":\"2026-04-09T10:00:00Z\",",
        "\"content\":\"<local-command-stdout>Compacted</local-command-stdout>\"}\n"
    );
    let entries = parse_transcript(content);
    assert_eq!(entries.len(), 1);
    assert!(
        matches!(entries[0], TranscriptEntry::LocalCommand { .. }),
        "<local-command-stdout> content should emit LocalCommand"
    );
}

#[test]
fn test_parse_transcript_command_name_emits_local_command() {
    let content = concat!(
        "{\"type\":\"user\",\"timestamp\":\"2026-04-09T10:00:00Z\",",
        "\"content\":\"<command-name>/compact</command-name>\"}\n"
    );
    let entries = parse_transcript(content);
    assert_eq!(entries.len(), 1);
    assert!(
        matches!(entries[0], TranscriptEntry::LocalCommand { .. }),
        "<command-name> content should emit LocalCommand"
    );
}

#[test]
fn test_last_non_assistant_entry_skips_local_command() {
    // LocalCommand between TurnCompleted and CompactBoundary: should be skipped,
    // so we see CompactBoundary as the last meaningful entry.
    let entries = vec![
        TranscriptEntry::TurnCompleted {
            timestamp: "2026-04-09T10:00:00Z".to_string(),
        },
        TranscriptEntry::CompactBoundary {
            timestamp: "2026-04-09T10:00:01Z".to_string(),
        },
        TranscriptEntry::LocalCommand {
            timestamp: "2026-04-09T10:00:02Z".to_string(),
        },
    ];
    // infer_activity uses last_non_assistant_entry; LocalCommand is excluded,
    // so we fall through to CompactBoundary → Compacting.
    assert_eq!(
        infer_activity_from_last_entry(&entries),
        Some(Activity::Compacting)
    );
}

// ── Compaction restore message ────────────────────────────────────

#[test]
fn test_normalize_compaction_restore_message_maps_to_done_non_stale() {
    let entries = vec![
        TranscriptEntry::CompactBoundary {
            timestamp: "2026-04-09T10:00:00Z".to_string(),
        },
        TranscriptEntry::User {
            timestamp: "2026-04-09T10:00:00Z".to_string(),
            content: "This session is being continued from a previous conversation that ran out of context. The summary below covers the earlier portion.".to_string(),
        },
    ];
    let normalized = normalize_claude_state(&entries, false);
    assert_eq!(normalized.status, NormalizedStatus::Done);
    assert_eq!(normalized.reason, Some(NormalizedReason::ReadyForNextTask));
}

#[test]
fn test_normalize_stale_compaction_restore_message_maps_to_done() {
    let entries = vec![
        TranscriptEntry::CompactBoundary {
            timestamp: "2026-04-09T10:00:00Z".to_string(),
        },
        TranscriptEntry::User {
            timestamp: "2026-04-09T10:00:00Z".to_string(),
            content: "This session is being continued from a previous conversation that ran out of context. The summary below covers the earlier portion.".to_string(),
        },
    ];
    let normalized = normalize_claude_state(&entries, true);
    assert_eq!(normalized.status, NormalizedStatus::Done);
    assert_eq!(normalized.reason, Some(NormalizedReason::ReadyForNextTask));
}

/// Full `/compact` sequence as written to the transcript: TurnCompleted (before
/// compact), then compact_boundary + restore + LocalCommand entries.  The final
/// state should be "done · ready for next task" — not "working" or "unknown".
#[test]
fn test_normalize_manual_compact_sequence_shows_done() {
    let entries = vec![
        TranscriptEntry::TurnCompleted {
            timestamp: "2026-04-09T09:12:12Z".to_string(),
        },
        TranscriptEntry::CompactBoundary {
            timestamp: "2026-04-09T09:16:10Z".to_string(),
        },
        TranscriptEntry::User {
            timestamp: "2026-04-09T09:16:10Z".to_string(),
            content: "This session is being continued from a previous conversation that ran out of context.".to_string(),
        },
        TranscriptEntry::LocalCommand {
            timestamp: "2026-04-09T09:14:44Z".to_string(),
        },
        TranscriptEntry::LocalCommand {
            timestamp: "2026-04-09T09:14:44Z".to_string(),
        },
        TranscriptEntry::LocalCommand {
            timestamp: "2026-04-09T09:16:10Z".to_string(),
        },
    ];

    // Non-stale: restore message is last meaningful entry → Done
    let normalized = normalize_claude_state(&entries, false);
    assert_eq!(normalized.status, NormalizedStatus::Done);
    assert_eq!(normalized.reason, Some(NormalizedReason::ReadyForNextTask));

    // Stale: same result
    let normalized_stale = normalize_claude_state(&entries, true);
    assert_eq!(normalized_stale.status, NormalizedStatus::Done);
    assert_eq!(
        normalized_stale.reason,
        Some(NormalizedReason::ReadyForNextTask)
    );
}

#[test]
fn test_infer_activity_manual_compact_restore_message_is_compacting() {
    let entries = vec![
        TranscriptEntry::CompactBoundary {
            timestamp: "2026-04-09T10:00:00Z".to_string(),
        },
        TranscriptEntry::User {
            timestamp: "2026-04-09T10:00:00Z".to_string(),
            content: "This session is being continued from a previous conversation that ran out of context.".to_string(),
        },
        TranscriptEntry::LocalCommand {
            timestamp: "2026-04-09T09:58:00Z".to_string(),
        },
    ];
    // LocalCommand is skipped, restore message is last → Compacting
    assert_eq!(
        infer_activity_from_last_entry(&entries),
        Some(Activity::Compacting)
    );
}

/// `/clear` with no message leaves a trailing LocalCommand as the last entry.
/// The session is idle — not "working" — so we should show Done.
#[test]
fn test_normalize_clear_command_shows_done_non_stale() {
    let entries = vec![
        TranscriptEntry::User {
            timestamp: "2026-04-09T10:00:00Z".to_string(),
            content: "some question".to_string(),
        },
        TranscriptEntry::LocalCommand {
            timestamp: "2026-04-09T10:00:01Z".to_string(),
        },
    ];
    let normalized = normalize_claude_state(&entries, false);
    assert_eq!(normalized.status, NormalizedStatus::Done);
    assert_eq!(normalized.reason, Some(NormalizedReason::ReadyForNextTask));
}

// ── scan_session_registry ─────────────────────────────────────────

#[test]
fn test_scan_session_registry_reads_valid_file() {
    use crate::detect::claude::scan_session_registry;
    use std::io::Write;

    let dir = tempfile::tempdir().expect("create temp dir");
    let file_path = dir.path().join("3846.json");
    let mut f = std::fs::File::create(&file_path).unwrap();
    writeln!(
        f,
        r#"{{"pid":3846,"sessionId":"abc-123","cwd":"/home/user/proj","startedAt":0,"kind":"interactive","entrypoint":"cli"}}"#
    )
    .unwrap();

    let map = scan_session_registry(dir.path());
    assert_eq!(map.len(), 1);
    let entry = map.get("abc-123").expect("should find by session_id");
    assert_eq!(entry.pid, 3846);
    assert_eq!(entry.session_id, "abc-123");
    assert_eq!(entry.cwd.to_str().unwrap(), "/home/user/proj");
    assert_eq!(entry.kind, "interactive");
}

#[test]
fn test_scan_session_registry_skips_invalid_files() {
    use crate::detect::claude::scan_session_registry;
    use std::io::Write;

    let dir = tempfile::tempdir().expect("create temp dir");
    // non-json extension
    let f1 = dir.path().join("README.txt");
    std::fs::File::create(&f1).unwrap();
    // invalid JSON
    let f2 = dir.path().join("9999.json");
    let mut f = std::fs::File::create(&f2).unwrap();
    writeln!(f, "not json").unwrap();
    // non-numeric pid
    let f3 = dir.path().join("foo.json");
    let mut f = std::fs::File::create(&f3).unwrap();
    writeln!(
        f,
        r#"{{"pid":0,"sessionId":"x","cwd":"/","kind":"interactive"}}"#
    )
    .unwrap();

    let map = scan_session_registry(dir.path());
    assert!(map.is_empty(), "no valid entries should be found");
}

#[test]
fn test_scan_session_registry_empty_dir() {
    use crate::detect::claude::scan_session_registry;

    let dir = tempfile::tempdir().expect("create temp dir");
    let map = scan_session_registry(dir.path());
    assert!(map.is_empty());
}

#[test]
fn test_scan_session_registry_nonexistent_dir() {
    use crate::detect::claude::scan_session_registry;

    let map = scan_session_registry(std::path::Path::new("/nonexistent/path/sessions"));
    assert!(
        map.is_empty(),
        "nonexistent directory should return empty map"
    );
}
