use ai_overview::detect::codex::*;

#[test]
fn test_parse_rollout_events() {
    let content = include_str!("fixtures/codex_rollout.jsonl");
    let events = parse_rollout(content);
    assert_eq!(events.len(), 8);
}

#[test]
fn test_extract_session_meta() {
    let content = include_str!("fixtures/codex_rollout.jsonl");
    let events = parse_rollout(content);
    let meta = extract_session_meta(&events);
    assert!(meta.is_some());
    let m = meta.unwrap();
    assert_eq!(m.id, "019d4422-test-uuid");
    assert_eq!(m.branch.as_deref(), Some("feat/new-feature"));
    assert_eq!(m.cwd, "/Users/dev/project");
    assert_eq!(m.source, "cli");
}

#[test]
fn test_extract_subagent_spawns() {
    let content = include_str!("fixtures/codex_rollout.jsonl");
    let events = parse_rollout(content);
    let subs = extract_subagent_spawns(&events);
    assert_eq!(subs.len(), 1);
    assert_eq!(subs[0].nickname, "Parfit");
    assert_eq!(subs[0].role, "worker");
    assert_eq!(subs[0].thread_id, "019d4423-sub-uuid");
}

#[test]
fn test_extract_tool_calls() {
    let content = include_str!("fixtures/codex_rollout.jsonl");
    let events = parse_rollout(content);
    let tools = extract_tool_calls(&events);
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].name, "shell");
}

#[test]
fn test_infer_rollout_status_complete() {
    let content = include_str!("fixtures/codex_rollout.jsonl");
    let events = parse_rollout(content);
    let status = infer_rollout_status(&events);
    assert_eq!(status, CodexInferredStatus::Done);
}

#[test]
fn test_infer_rollout_status_running() {
    let content = r#"{"timestamp":"2026-03-31T10:00:00.000Z","type":"session_meta","payload":{"id":"test","cwd":"/tmp","cli_version":"0.1","source":"cli","model_provider":"openai","git":{"sha":"abc","branch":"main","origin_url":""}}}
{"timestamp":"2026-03-31T10:00:01.000Z","type":"event_msg","payload":{"type":"task_started","turn_id":"turn_1"}}"#;
    let events = parse_rollout(content);
    let status = infer_rollout_status(&events);
    assert_eq!(status, CodexInferredStatus::Running);
}

#[test]
fn test_parse_session_index() {
    let content = include_str!("fixtures/codex_session_index.jsonl");
    let entries = parse_session_index(content);
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0].id, "019d4422-test-uuid");
    assert_eq!(entries[0].thread_name, "Implement new validation feature");
}

#[test]
fn test_parse_codex_source_subagent() {
    let source = r#"{"subagent":{"thread_spawn":{"parent_thread_id":"019d4422-parent","depth":1,"agent_nickname":"Nash","agent_role":"worker"}}}"#;
    let info = parse_source_field(source);
    assert!(info.is_subagent);
    assert_eq!(info.parent_thread_id.as_deref(), Some("019d4422-parent"));
    assert_eq!(info.agent_nickname.as_deref(), Some("Nash"));
}

#[test]
fn test_parse_codex_source_cli() {
    let info = parse_source_field("cli");
    assert!(!info.is_subagent);
    assert!(info.parent_thread_id.is_none());
}
