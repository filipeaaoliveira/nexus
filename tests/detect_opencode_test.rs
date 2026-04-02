use ai_overview::detect::opencode::*;
use ai_overview::types::PrimaryStatus;
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
fn test_waiting_status_when_assistant_done_no_running_parts() {
    // When the assistant turn is completed and no parts are running or pending,
    // the status should be Waiting.
    let parts = vec![
        PartData {
            part_type: "tool".to_string(),
            tool_name: Some("bash".to_string()),
            tool_status: Some("completed".to_string()),
            tool_input_summary: None,
            time_created: 2000,
        },
        PartData {
            part_type: "text".to_string(),
            tool_name: None,
            tool_status: None,
            tool_input_summary: None,
            time_created: 1000,
        },
    ];

    let inferred = infer_status_from_parts(&parts);
    // assistant_turn_completed = true → Waiting
    let status = resolve_status(inferred, &parts, true);

    assert_eq!(status, PrimaryStatus::Waiting);
}

#[test]
fn test_running_status_when_session_is_fresh() {
    // Even if last message is assistant and no running parts, a fresh session
    // means the agent is still actively working → Running, not Waiting.
    let parts = vec![PartData {
        part_type: "tool".to_string(),
        tool_name: Some("bash".to_string()),
        tool_status: Some("completed".to_string()),
        tool_input_summary: None,
        time_created: 2000,
    }];

    let inferred = infer_status_from_parts(&parts);
    // assistant_turn_completed = false → still Running
    let status = resolve_status(inferred, &parts, false);

    assert_eq!(status, PrimaryStatus::Running);
}

#[test]
fn test_running_status_when_parts_are_running() {
    let parts = vec![PartData {
        part_type: "tool".to_string(),
        tool_name: Some("bash".to_string()),
        tool_status: Some("running".to_string()),
        tool_input_summary: None,
        time_created: 2000,
    }];

    // Even if assistant_turn_completed is true, a running part means Running.
    let inferred = infer_status_from_parts(&parts);
    let status = resolve_status(inferred, &parts, true);

    assert_eq!(status, PrimaryStatus::Running);
}

#[test]
fn test_running_status_when_parts_are_pending() {
    // A pending tool call also means the agent is working, not waiting.
    let parts = vec![PartData {
        part_type: "tool".to_string(),
        tool_name: Some("write".to_string()),
        tool_status: Some("pending".to_string()),
        tool_input_summary: None,
        time_created: 2000,
    }];

    let inferred = infer_status_from_parts(&parts);
    let status = resolve_status(inferred, &parts, true);

    assert_eq!(status, PrimaryStatus::Running);
}

#[test]
fn test_error_status_preserved_even_with_assistant_role() {
    let parts = vec![PartData {
        part_type: "error".to_string(),
        tool_name: None,
        tool_status: None,
        tool_input_summary: None,
        time_created: 3000,
    }];

    let inferred = infer_status_from_parts(&parts);
    let status = resolve_status(inferred, &parts, true);

    assert_eq!(status, PrimaryStatus::Error);
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
