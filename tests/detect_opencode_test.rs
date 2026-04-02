use ai_overview::detect::opencode::*;
use ai_overview::event::AgentEvent;
use ai_overview::types::{Activity, AgentTool, PrimaryStatus, TodoPriority, TodoStatus};
use std::path::PathBuf;

/// Create a temporary SQLite DB with the `message` table for testing.
fn setup_message_db() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let conn = rusqlite::Connection::open(&db_path).unwrap();
    conn.execute_batch(
        "CREATE TABLE message (
            id TEXT PRIMARY KEY,
            session_id TEXT NOT NULL,
            data TEXT NOT NULL,
            time_created INTEGER NOT NULL
        );",
    )
    .unwrap();
    drop(conn);
    (dir, db_path)
}

#[test]
fn test_parse_message_data_user() {
    let json = r#"{"role":"user","time":{"created":1774988071935},"agent":"general","model":{"providerID":"github-copilot","modelID":"claude-opus-4.6"}}"#;
    let data = parse_message_data(json);
    assert!(data.is_some());
    let d = data.unwrap();
    assert_eq!(d.role, "user");
    assert_eq!(d.agent, Some("general".to_string()));
    assert_eq!(d.model_id, Some("claude-opus-4.6".to_string()));
}

#[test]
fn test_parse_message_data_assistant() {
    let json = r#"{"role":"assistant","parentID":"msg_abc","mode":"general","agent":"explore","path":{"cwd":"/tmp","root":"/tmp"},"modelID":"claude-opus-4.6","providerID":"github-copilot","time":{"created":1774988071939}}"#;
    let data = parse_message_data(json);
    assert!(data.is_some());
    let d = data.unwrap();
    assert_eq!(d.role, "assistant");
    assert_eq!(d.agent, Some("explore".to_string()));
}

#[test]
fn test_parse_part_data_tool() {
    let json = r#"{"type":"tool","tool":"bash","callID":"tooluse_abc","state":{"status":"completed","input":{"command":"npm test"},"output":"All tests passed"}}"#;
    let part = parse_part_data(json, 1774988071935);
    assert!(part.is_some());
    let p = part.unwrap();
    assert_eq!(p.part_type, "tool");
    assert_eq!(p.tool_name, Some("bash".to_string()));
    assert_eq!(p.tool_status, Some("completed".to_string()));
    assert_eq!(p.tool_input_summary, Some("npm test".to_string()));
    assert_eq!(p.time_created, 1774988071935);
}

#[test]
fn test_parse_part_data_text() {
    let json =
        r#"{"type":"text","text":"I'll help you with that.","time":{"start":1774988083425}}"#;
    let part = parse_part_data(json, 0);
    assert!(part.is_some());
    let p = part.unwrap();
    assert_eq!(p.part_type, "text");
}

#[test]
fn test_parse_part_data_step_finish() {
    let json = r#"{"type":"step-finish","snapshot":"abc123","reason":"tool-calls","tokens":{"total":34898},"cost":0}"#;
    let part = parse_part_data(json, 0);
    assert!(part.is_some());
    let p = part.unwrap();
    assert_eq!(p.part_type, "step-finish");
}

#[test]
fn test_session_is_subagent() {
    assert!(is_subagent_title("Fix the tests (@explore subagent)"));
    assert!(is_subagent_title("Search codebase (@general subagent)"));
    assert!(!is_subagent_title("Fix the tests"));
    assert!(!is_subagent_title("Regular session title"));
}

#[test]
fn test_extract_subagent_type() {
    assert_eq!(
        extract_subagent_type("Fix tests (@explore subagent)"),
        Some("explore")
    );
    assert_eq!(
        extract_subagent_type("Search (@general subagent)"),
        Some("general")
    );
    assert_eq!(extract_subagent_type("Normal title"), None);
}

#[test]
fn test_resolve_status_running_tool() {
    let parts = vec![PartData {
        part_type: "tool".to_string(),
        tool_name: Some("bash".to_string()),
        tool_status: Some("running".to_string()),
        tool_input_summary: None,
        time_created: 2000,
    }];
    assert_eq!(resolve_status(&parts, false), PrimaryStatus::Running);
}

#[test]
fn test_resolve_status_pending_tool() {
    let parts = vec![PartData {
        part_type: "tool".to_string(),
        tool_name: Some("write".to_string()),
        tool_status: Some("pending".to_string()),
        tool_input_summary: None,
        time_created: 2000,
    }];
    assert_eq!(resolve_status(&parts, false), PrimaryStatus::Running);
}

#[test]
fn test_resolve_status_completed_tool() {
    let parts = vec![PartData {
        part_type: "tool".to_string(),
        tool_name: Some("bash".to_string()),
        tool_status: Some("completed".to_string()),
        tool_input_summary: None,
        time_created: 2000,
    }];
    assert_eq!(resolve_status(&parts, false), PrimaryStatus::Running);
}

#[test]
fn test_resolve_status_step_start() {
    let parts = vec![PartData {
        part_type: "step-start".to_string(),
        tool_name: None,
        tool_status: None,
        tool_input_summary: None,
        time_created: 2000,
    }];
    assert_eq!(resolve_status(&parts, false), PrimaryStatus::Running);
}

#[test]
fn test_resolve_status_text() {
    let parts = vec![PartData {
        part_type: "text".to_string(),
        tool_name: None,
        tool_status: None,
        tool_input_summary: None,
        time_created: 2000,
    }];
    assert_eq!(resolve_status(&parts, false), PrimaryStatus::Running);
}

#[test]
fn test_resolve_status_compaction() {
    let parts = vec![PartData {
        part_type: "compaction".to_string(),
        tool_name: None,
        tool_status: None,
        tool_input_summary: None,
        time_created: 2000,
    }];
    assert_eq!(resolve_status(&parts, false), PrimaryStatus::Running);
}

#[test]
fn test_resolve_status_step_finish_turn_not_completed() {
    let parts = vec![PartData {
        part_type: "step-finish".to_string(),
        tool_name: None,
        tool_status: None,
        tool_input_summary: None,
        time_created: 2000,
    }];
    assert_eq!(resolve_status(&parts, false), PrimaryStatus::Running);
}

#[test]
fn test_resolve_status_step_finish_turn_completed() {
    let parts = vec![PartData {
        part_type: "step-finish".to_string(),
        tool_name: None,
        tool_status: None,
        tool_input_summary: None,
        time_created: 2000,
    }];
    assert_eq!(resolve_status(&parts, true), PrimaryStatus::Waiting);
}

#[test]
fn test_resolve_status_no_parts_turn_completed() {
    let parts: Vec<PartData> = vec![];
    assert_eq!(resolve_status(&parts, true), PrimaryStatus::Waiting);
}

#[test]
fn test_resolve_status_no_parts_turn_not_completed() {
    let parts: Vec<PartData> = vec![];
    assert_eq!(resolve_status(&parts, false), PrimaryStatus::Running);
}

#[test]
fn test_resolve_status_running_tool_deep_overrides_step_finish() {
    // A running tool deeper in the list should override a step-finish at the top
    let parts = vec![
        PartData {
            part_type: "step-finish".to_string(),
            tool_name: None,
            tool_status: None,
            tool_input_summary: None,
            time_created: 3000,
        },
        PartData {
            part_type: "tool".to_string(),
            tool_name: Some("bash".to_string()),
            tool_status: Some("running".to_string()),
            tool_input_summary: None,
            time_created: 2000,
        },
    ];
    assert_eq!(resolve_status(&parts, true), PrimaryStatus::Running);
}

#[test]
fn test_resolve_status_reasoning() {
    let parts = vec![PartData {
        part_type: "reasoning".to_string(),
        tool_name: None,
        tool_status: None,
        tool_input_summary: None,
        time_created: 2000,
    }];
    assert_eq!(resolve_status(&parts, false), PrimaryStatus::Running);
}

#[test]
fn test_resolve_status_running_tool_overrides_turn_completed() {
    // Even if assistant_turn_completed is true, a running tool means Running
    let parts = vec![PartData {
        part_type: "tool".to_string(),
        tool_name: Some("bash".to_string()),
        tool_status: Some("running".to_string()),
        tool_input_summary: None,
        time_created: 2000,
    }];
    assert_eq!(resolve_status(&parts, true), PrimaryStatus::Running);
}

#[test]
fn test_retry_maps_to_working_retrying() {
    use ai_overview::types::{NormalizedReason, NormalizedStatus};

    let state = normalize_opencode_state(
        Some("retry"),
        None,
        &[PartData {
            part_type: "retry".to_string(),
            tool_name: None,
            tool_status: None,
            tool_input_summary: None,
            time_created: 1000,
        }],
        false,
        None,
    );

    assert_eq!(state.status, NormalizedStatus::Working);
    assert_eq!(state.reason, Some(NormalizedReason::Retrying));
}

#[test]
fn test_completed_review_maps_to_needs_input_review() {
    use ai_overview::types::{NormalizedReason, NormalizedStatus};

    let state = normalize_opencode_state(
        Some("idle"),
        None,
        &[PartData {
            part_type: "step-finish".to_string(),
            tool_name: None,
            tool_status: None,
            tool_input_summary: None,
            time_created: 1000,
        }],
        true,
        Some("### Issues\n#### Critical"),
    );

    assert_eq!(state.status, NormalizedStatus::NeedsInput);
    assert_eq!(state.reason, Some(NormalizedReason::Review));
}

#[test]
fn test_assistant_turn_completed_no_messages() {
    let (_dir, db_path) = setup_message_db();
    let result = query_assistant_turn_completed(&db_path, "nonexistent");
    assert!(result.is_ok());
    assert!(!result.unwrap());
}

#[test]
fn test_assistant_turn_completed_null_timestamp() {
    let (_dir, db_path) = setup_message_db();
    let conn = rusqlite::Connection::open(&db_path).unwrap();
    conn.execute(
        "INSERT INTO message (id, session_id, data, time_created) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![
            "msg1",
            "sess1",
            r#"{"role":"assistant","time":{"created":1000}}"#,
            1000
        ],
    )
    .unwrap();
    drop(conn);

    let result = query_assistant_turn_completed(&db_path, "sess1");
    assert!(result.is_ok());
    assert!(
        !result.unwrap(),
        "Should be false when time.completed is absent"
    );
}

#[test]
fn test_assistant_turn_completed_with_timestamp() {
    let (_dir, db_path) = setup_message_db();
    let conn = rusqlite::Connection::open(&db_path).unwrap();
    conn.execute(
        "INSERT INTO message (id, session_id, data, time_created) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![
            "msg1",
            "sess1",
            r#"{"role":"assistant","time":{"created":1000,"completed":2000}}"#,
            1000
        ],
    )
    .unwrap();
    drop(conn);

    let result = query_assistant_turn_completed(&db_path, "sess1");
    assert!(result.is_ok());
    assert!(result.unwrap(), "Should be true when time.completed is set");
}

#[test]
fn test_compaction_part_detected() {
    let parts = vec![PartData {
        part_type: "compaction".to_string(),
        tool_name: None,
        tool_status: None,
        tool_input_summary: None,
        time_created: 1000,
    }];
    let activity = infer_activity_from_parts(&parts);
    assert_eq!(activity, Some(Activity::Compacting));
}

#[test]
fn test_non_compaction_part_not_affected() {
    let parts = vec![PartData {
        part_type: "text".to_string(),
        tool_name: None,
        tool_status: None,
        tool_input_summary: None,
        time_created: 1000,
    }];
    let activity = infer_activity_from_parts(&parts);
    assert_eq!(activity, Some(Activity::Generating));
}

#[test]
fn test_infer_activity_running_tool_in_non_latest_position() {
    // A running tool deeper in the list should be detected as ToolCall,
    // even if the latest part is a text part.
    let parts = vec![
        PartData {
            part_type: "text".to_string(),
            tool_name: None,
            tool_status: None,
            tool_input_summary: None,
            time_created: 3000,
        },
        PartData {
            part_type: "tool".to_string(),
            tool_name: Some("bash".to_string()),
            tool_status: Some("running".to_string()),
            tool_input_summary: None,
            time_created: 2000,
        },
    ];
    assert_eq!(infer_activity_from_parts(&parts), Some(Activity::ToolCall));
}

#[test]
fn test_infer_activity_pending_tool_in_non_latest_position() {
    let parts = vec![
        PartData {
            part_type: "text".to_string(),
            tool_name: None,
            tool_status: None,
            tool_input_summary: None,
            time_created: 3000,
        },
        PartData {
            part_type: "tool".to_string(),
            tool_name: Some("write".to_string()),
            tool_status: Some("pending".to_string()),
            tool_input_summary: None,
            time_created: 2000,
        },
    ];
    assert_eq!(infer_activity_from_parts(&parts), Some(Activity::ToolCall));
}

#[test]
fn test_infer_activity_completed_tool_overrides_older_running_tool() {
    let parts = vec![
        PartData {
            part_type: "text".to_string(),
            tool_name: None,
            tool_status: None,
            tool_input_summary: None,
            time_created: 4000,
        },
        PartData {
            part_type: "tool".to_string(),
            tool_name: Some("bash".to_string()),
            tool_status: Some("completed".to_string()),
            tool_input_summary: None,
            time_created: 3000,
        },
        PartData {
            part_type: "tool".to_string(),
            tool_name: Some("bash".to_string()),
            tool_status: Some("running".to_string()),
            tool_input_summary: None,
            time_created: 2000,
        },
    ];
    assert_eq!(
        infer_activity_from_parts(&parts),
        Some(Activity::Generating)
    );
}

#[test]
fn test_infer_activity_reasoning_part() {
    let parts = vec![PartData {
        part_type: "reasoning".to_string(),
        tool_name: None,
        tool_status: None,
        tool_input_summary: None,
        time_created: 1000,
    }];
    assert_eq!(infer_activity_from_parts(&parts), Some(Activity::Thinking));
}

// ── Todo query tests ──────────────────────────────────────────────

/// Helper: create a temp DB with the `todo` table.
fn setup_todo_db(dir: &std::path::Path) -> std::path::PathBuf {
    let db_path = dir.join("opencode.db");
    let conn = rusqlite::Connection::open(&db_path).unwrap();
    conn.execute_batch(
        "CREATE TABLE todo (
            session_id TEXT NOT NULL,
            content TEXT NOT NULL,
            status TEXT NOT NULL,
            priority TEXT NOT NULL,
            position INTEGER NOT NULL,
            time_created INTEGER NOT NULL,
            time_updated INTEGER NOT NULL,
            PRIMARY KEY (session_id, position)
        );",
    )
    .unwrap();
    db_path
}

#[test]
fn test_query_session_todos_empty() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = setup_todo_db(dir.path());
    let todos = query_session_todos(&db_path, "sess-1").unwrap();
    assert!(todos.is_empty());
}

#[test]
fn test_query_session_todos_basic() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = setup_todo_db(dir.path());
    let conn = rusqlite::Connection::open(&db_path).unwrap();
    conn.execute(
        "INSERT INTO todo (session_id, content, status, priority, position, time_created, time_updated) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params!["sess-1", "Fix bug", "in_progress", "high", 0, 1000, 1000],
    ).unwrap();
    conn.execute(
        "INSERT INTO todo (session_id, content, status, priority, position, time_created, time_updated) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params!["sess-1", "Write tests", "pending", "medium", 1, 1001, 1001],
    ).unwrap();

    let todos = query_session_todos(&db_path, "sess-1").unwrap();
    assert_eq!(todos.len(), 2);
    assert_eq!(todos[0].content, "Fix bug");
    assert_eq!(todos[0].status, TodoStatus::InProgress);
    assert_eq!(todos[0].priority, TodoPriority::High);
    assert_eq!(todos[1].content, "Write tests");
    assert_eq!(todos[1].status, TodoStatus::Pending);
    assert_eq!(todos[1].priority, TodoPriority::Medium);
}

#[test]
fn test_query_session_todos_filters_by_session() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = setup_todo_db(dir.path());
    let conn = rusqlite::Connection::open(&db_path).unwrap();
    conn.execute(
        "INSERT INTO todo (session_id, content, status, priority, position, time_created, time_updated) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params!["sess-1", "Task A", "pending", "high", 0, 1000, 1000],
    ).unwrap();
    conn.execute(
        "INSERT INTO todo (session_id, content, status, priority, position, time_created, time_updated) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params!["sess-2", "Task B", "completed", "low", 0, 1000, 1000],
    ).unwrap();

    let todos = query_session_todos(&db_path, "sess-1").unwrap();
    assert_eq!(todos.len(), 1);
    assert_eq!(todos[0].content, "Task A");
}

#[test]
fn test_query_session_todos_ordered_by_position() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = setup_todo_db(dir.path());
    let conn = rusqlite::Connection::open(&db_path).unwrap();
    conn.execute(
        "INSERT INTO todo (session_id, content, status, priority, position, time_created, time_updated) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params!["sess-1", "Third", "pending", "low", 2, 1000, 1000],
    ).unwrap();
    conn.execute(
        "INSERT INTO todo (session_id, content, status, priority, position, time_created, time_updated) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params!["sess-1", "First", "completed", "high", 0, 1000, 1000],
    ).unwrap();
    conn.execute(
        "INSERT INTO todo (session_id, content, status, priority, position, time_created, time_updated) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params!["sess-1", "Second", "in_progress", "medium", 1, 1000, 1000],
    ).unwrap();

    let todos = query_session_todos(&db_path, "sess-1").unwrap();
    assert_eq!(todos.len(), 3);
    assert_eq!(todos[0].content, "First");
    assert_eq!(todos[1].content, "Second");
    assert_eq!(todos[2].content, "Third");
}

#[test]
fn test_query_session_todos_unknown_status_defaults_to_pending() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = setup_todo_db(dir.path());
    let conn = rusqlite::Connection::open(&db_path).unwrap();
    conn.execute(
        "INSERT INTO todo (session_id, content, status, priority, position, time_created, time_updated) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params!["sess-1", "Mystery", "some_new_status", "medium", 0, 1000, 1000],
    ).unwrap();

    let todos = query_session_todos(&db_path, "sess-1").unwrap();
    assert_eq!(todos[0].status, TodoStatus::Pending);
}

#[test]
fn test_query_session_todos_unknown_priority_defaults_to_medium() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = setup_todo_db(dir.path());
    let conn = rusqlite::Connection::open(&db_path).unwrap();
    conn.execute(
        "INSERT INTO todo (session_id, content, status, priority, position, time_created, time_updated) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params!["sess-1", "Mystery", "pending", "critical", 0, 1000, 1000],
    ).unwrap();

    let todos = query_session_todos(&db_path, "sess-1").unwrap();
    assert_eq!(todos[0].priority, TodoPriority::Medium);
}

// ── Fix #2: connection-reuse variants ────────────────────────────────────────

/// Helper: create a full temp DB with session, todo, and part tables.
fn setup_full_db(dir: &std::path::Path) -> std::path::PathBuf {
    let db_path = dir.join("opencode.db");
    let conn = rusqlite::Connection::open(&db_path).unwrap();
    conn.execute_batch(
        "CREATE TABLE session (
            id TEXT PRIMARY KEY,
            project_id TEXT NOT NULL DEFAULT 'proj',
            parent_id TEXT,
            slug TEXT NOT NULL DEFAULT '',
            directory TEXT,
            title TEXT,
            version TEXT NOT NULL DEFAULT '1',
            share_url TEXT,
            summary_additions INTEGER,
            summary_deletions INTEGER,
            summary_files INTEGER,
            summary_diffs TEXT,
            revert TEXT,
            permission TEXT,
            time_created INTEGER NOT NULL,
            time_updated INTEGER NOT NULL,
            time_compacting INTEGER,
            time_archived INTEGER,
            workspace_id TEXT
        );
        CREATE TABLE todo (
            session_id TEXT NOT NULL,
            content TEXT NOT NULL,
            status TEXT NOT NULL,
            priority TEXT NOT NULL,
            position INTEGER NOT NULL,
            time_created INTEGER NOT NULL,
            time_updated INTEGER NOT NULL,
            PRIMARY KEY (session_id, position)
        );
        CREATE TABLE part (
            id TEXT PRIMARY KEY,
            session_id TEXT NOT NULL,
            message_id TEXT NOT NULL DEFAULT '',
            data TEXT NOT NULL,
            time_created INTEGER NOT NULL
        );",
    )
    .unwrap();
    db_path
}

#[test]
fn test_query_session_todos_conn_returns_same_as_path_variant() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = setup_full_db(dir.path());
    let conn = rusqlite::Connection::open(&db_path).unwrap();
    conn.execute(
        "INSERT INTO todo (session_id, content, status, priority, position, time_created, time_updated) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params!["sess-1", "Fix bug", "in_progress", "high", 0, 1000, 1000],
    ).unwrap();

    // via_path uses a new connection internally
    let via_path = query_session_todos(&db_path, "sess-1").unwrap();
    // via_conn reuses the open connection
    let via_conn = query_session_todos_conn(&conn, "sess-1").unwrap();

    assert_eq!(via_path, via_conn);
}

#[test]
fn test_query_subagent_sessions_conn_returns_same_as_path_variant() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = setup_full_db(dir.path());
    let conn = rusqlite::Connection::open(&db_path).unwrap();
    conn.execute(
        "INSERT INTO session (id, parent_id, title, time_created, time_updated) VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params!["child-1", "parent-1", "Do task (@explore subagent)", 1000, 2000],
    ).unwrap();
    conn.execute(
        "INSERT INTO session (id, parent_id, title, time_created, time_updated) VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params!["child-2", "parent-1", "Other task (@general subagent)", 1001, 2001],
    ).unwrap();
    conn.execute(
        "INSERT INTO session (id, title, time_created, time_updated) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params!["unrelated", "Unrelated session", 1002, 2002],
    )
    .unwrap();

    let via_path = query_subagent_sessions(&db_path, "parent-1").unwrap();
    let via_conn = query_subagent_sessions_conn(&conn, "parent-1").unwrap();

    assert_eq!(via_path.len(), 2);
    assert_eq!(via_conn.len(), 2);
    assert_eq!(via_path[0].id, via_conn[0].id);
    assert_eq!(via_path[1].id, via_conn[1].id);
}

// ── Fix #4+#5: SubagentCompleted deduplication & HashSet ─────────────────────

fn make_subagent(id: &str, title: &str, archived: bool) -> OpenCodeSession {
    OpenCodeSession {
        id: id.to_string(),
        title: Some(title.to_string()),
        directory: None,
        parent_id: Some("parent".to_string()),
        time_created: 1000,
        time_updated: 2000,
        time_archived: if archived { Some(3000) } else { None },
        branch: None,
    }
}

#[test]
fn test_collect_subagent_events_new_subagent_emits_spawned() {
    let mut state = SessionEmitState::default();
    let sub = make_subagent("sub-1", "Do task (@explore subagent)", false);

    let events = collect_subagent_events(&[sub], &mut state, AgentTool::OpenCode, 42);

    assert_eq!(events.len(), 1, "should emit exactly one SubagentSpawned");
    assert!(
        matches!(&events[0], AgentEvent::SubagentSpawned { .. }),
        "event should be SubagentSpawned"
    );
}

#[test]
fn test_collect_subagent_events_known_active_subagent_emits_nothing() {
    let mut state = SessionEmitState::default();
    let sub = make_subagent("sub-1", "Do task (@explore subagent)", false);

    // First poll: spawned
    collect_subagent_events(&[sub.clone()], &mut state, AgentTool::OpenCode, 42);
    // Second poll: still active, should emit nothing
    let events = collect_subagent_events(&[sub], &mut state, AgentTool::OpenCode, 42);

    assert!(
        events.is_empty(),
        "known active subagent should produce no events"
    );
}

#[test]
fn test_collect_subagent_events_completed_emitted_only_once() {
    let mut state = SessionEmitState::default();
    let active = make_subagent("sub-1", "Do task (@explore subagent)", false);
    let archived = make_subagent("sub-1", "Do task (@explore subagent)", true);

    // Poll 1: spawned
    collect_subagent_events(&[active], &mut state, AgentTool::OpenCode, 42);
    // Poll 2: archived → SubagentCompleted
    let events2 = collect_subagent_events(&[archived.clone()], &mut state, AgentTool::OpenCode, 42);
    assert_eq!(events2.len(), 1, "poll 2 should emit SubagentCompleted");
    assert!(matches!(&events2[0], AgentEvent::SubagentCompleted { .. }));

    // Poll 3: still archived → NO additional SubagentCompleted
    let events3 = collect_subagent_events(&[archived], &mut state, AgentTool::OpenCode, 42);
    assert!(
        events3.is_empty(),
        "SubagentCompleted must not be emitted again on subsequent polls"
    );
}

#[test]
fn test_collect_subagent_events_already_archived_on_first_sight_emits_spawned_and_completed() {
    // Edge case: we first observe a subagent that is already archived.
    // It should emit SubagentSpawned once, then SubagentCompleted once.
    let mut state = SessionEmitState::default();
    let archived = make_subagent("sub-1", "Do task (@explore subagent)", true);

    let events1 = collect_subagent_events(&[archived.clone()], &mut state, AgentTool::OpenCode, 42);
    // The subagent was never seen as active: depends on desired behavior.
    // We expect SubagentSpawned to still be emitted (it's the first time we see it),
    // but SubagentCompleted NOT yet (the "completed" check runs on the next poll
    // for the already-emitted set, not for newly-added ones).
    assert_eq!(events1.len(), 1);
    assert!(matches!(&events1[0], AgentEvent::SubagentSpawned { .. }));

    // Second poll: now it is in emitted set and archived → SubagentCompleted once
    let events2 = collect_subagent_events(&[archived.clone()], &mut state, AgentTool::OpenCode, 42);
    assert_eq!(events2.len(), 1);
    assert!(matches!(&events2[0], AgentEvent::SubagentCompleted { .. }));

    // Third poll: no more events
    let events3 = collect_subagent_events(&[archived], &mut state, AgentTool::OpenCode, 42);
    assert!(events3.is_empty());
}
