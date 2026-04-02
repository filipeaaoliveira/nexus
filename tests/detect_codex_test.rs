use ai_overview::detect::codex::*;
use ai_overview::detect::registry::AgentRegistry;
use ai_overview::types::{AgentId, AgentTool};

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

#[test]
fn test_build_codex_snapshot_classifies_top_level_and_child_threads() {
    let registry = AgentRegistry::new();
    let threads = vec![
        CodexThread {
            id: "parent-thread".into(),
            title: Some("Parent".into()),
            cwd: Some("/tmp/project".into()),
            source: Some("cli".into()),
            model: Some("gpt-5".into()),
            git_branch: Some("main".into()),
            agent_nickname: None,
            agent_role: None,
            created_at: 10,
            updated_at: 20,
            archived: 0,
            tokens_used: None,
            rollout_path: None,
        },
        CodexThread {
            id: "child-thread".into(),
            title: Some("Child".into()),
            cwd: Some("/tmp/project".into()),
            source: Some(
                r#"{"subagent":{"thread_spawn":{"parent_thread_id":"parent-thread","depth":1,"agent_nickname":"Nash","agent_role":"worker"}}}"#
                    .into(),
            ),
            model: Some("gpt-5".into()),
            git_branch: Some("main".into()),
            agent_nickname: Some("Nash".into()),
            agent_role: Some("worker".into()),
            created_at: 11,
            updated_at: 21,
            archived: 0,
            tokens_used: None,
            rollout_path: None,
        },
        CodexThread {
            id: "edge-only-child".into(),
            title: Some("Edge child".into()),
            cwd: Some("/tmp/project".into()),
            source: Some("cli".into()),
            model: Some("gpt-5".into()),
            git_branch: Some("main".into()),
            agent_nickname: None,
            agent_role: None,
            created_at: 12,
            updated_at: 22,
            archived: 0,
            tokens_used: None,
            rollout_path: None,
        },
    ];
    let edges = vec![
        SpawnEdge {
            parent_thread_id: "parent-thread".into(),
            child_thread_id: "child-thread".into(),
            status: "running".into(),
        },
        SpawnEdge {
            parent_thread_id: "parent-thread".into(),
            child_thread_id: "edge-only-child".into(),
            status: "running".into(),
        },
    ];

    let snapshot = build_codex_snapshot(&threads, &edges, &registry);

    assert_eq!(snapshot.top_level_threads.len(), 1);
    assert_eq!(snapshot.child_threads.len(), 2);

    let parent = &snapshot.top_level_threads[0];
    assert_eq!(parent.thread.id, "parent-thread");
    assert_eq!(parent.classification, CodexThreadClassification::TopLevel);
    assert_eq!(
        parent.agent_id,
        AgentId::session(AgentTool::Codex, "parent-thread")
    );

    let source_child = snapshot
        .child_threads
        .iter()
        .find(|thread| thread.thread.id == "child-thread")
        .unwrap();
    assert_eq!(
        source_child.classification,
        CodexThreadClassification::Child {
            parent_thread_id: Some("parent-thread".into()),
        }
    );

    let edge_child = snapshot
        .child_threads
        .iter()
        .find(|thread| thread.thread.id == "edge-only-child")
        .unwrap();
    assert_eq!(
        edge_child.classification,
        CodexThreadClassification::Child {
            parent_thread_id: Some("parent-thread".into()),
        }
    );
}

#[test]
fn test_build_codex_snapshot_keeps_same_cwd_top_level_threads_distinct() {
    let registry = AgentRegistry::new();
    let threads = vec![
        CodexThread {
            id: "thread-a".into(),
            title: Some("A".into()),
            cwd: Some("/tmp/project".into()),
            source: Some("cli".into()),
            model: Some("gpt-5".into()),
            git_branch: Some("main".into()),
            agent_nickname: None,
            agent_role: None,
            created_at: 10,
            updated_at: 30,
            archived: 0,
            tokens_used: None,
            rollout_path: None,
        },
        CodexThread {
            id: "thread-b".into(),
            title: Some("B".into()),
            cwd: Some("/tmp/project".into()),
            source: Some("cli".into()),
            model: Some("gpt-5".into()),
            git_branch: Some("main".into()),
            agent_nickname: None,
            agent_role: None,
            created_at: 11,
            updated_at: 31,
            archived: 0,
            tokens_used: None,
            rollout_path: None,
        },
    ];

    let snapshot = build_codex_snapshot(&threads, &[], &registry);

    assert_eq!(snapshot.top_level_threads.len(), 2);
    assert!(snapshot.child_threads.is_empty());
    assert_eq!(snapshot.top_level_threads[0].thread.id, "thread-a");
    assert_eq!(snapshot.top_level_threads[1].thread.id, "thread-b");
}
