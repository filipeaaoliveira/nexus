use nexus::detect::codex::*;
use nexus::detect::registry::AgentRegistry;
use nexus::domain::agents::{
    Activity, AgentId, AgentTool, NormalizedReason, NormalizedStatus, PrimaryStatus, TodoItem,
    TodoPriority, TodoStatus,
};
use nexus::domain::shared::from_epoch_secs;
use nexus::domain::system::CodexBackendHealth;
use nexus::event::AgentEvent;
use rusqlite::Connection;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn observed_session(
    session_id: &str,
    pid: Option<u32>,
    status: PrimaryStatus,
) -> ObservedCodexSession {
    ObservedCodexSession {
        agent_id: AgentId::session(AgentTool::Codex, session_id),
        pid,
        cwd: PathBuf::from("/tmp/project"),
        branch: Some("main".to_string()),
        started_at: from_epoch_secs(10),
        status,
        final_output: None,
        final_output_is_completion: false,
        latest_tool_call: None,
        todos: Vec::new(),
        subagents: Vec::new(),
    }
}

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
fn test_extract_latest_plan_update_todos() {
    let content = r#"{"timestamp":"2026-04-02T15:24:40.322Z","type":"response_item","payload":{"type":"function_call","name":"update_plan","arguments":"{\"plan\":[{\"step\":\"Read files\",\"status\":\"completed\"},{\"step\":\"Write tests\",\"status\":\"in_progress\"},{\"step\":\"Run full suite\",\"status\":\"pending\"}],\"explanation\":\"test\"}","call_id":"call_1"}}
{"timestamp":"2026-04-02T15:28:28.131Z","type":"response_item","payload":{"type":"function_call","name":"update_plan","arguments":"{\"plan\":[{\"step\":\"Read files\",\"status\":\"completed\"},{\"step\":\"Write tests\",\"status\":\"completed\"},{\"step\":\"Run full suite\",\"status\":\"in_progress\"}],\"explanation\":\"newer\"}","call_id":"call_2"}}"#;
    let events = parse_rollout(content);
    let todos = extract_plan_todos(&events);

    assert_eq!(todos.len(), 3);
    assert_eq!(todos[0].content, "Read files");
    assert_eq!(todos[0].status, TodoStatus::Completed);
    assert_eq!(todos[0].priority, TodoPriority::Medium);
    assert_eq!(todos[1].content, "Write tests");
    assert_eq!(todos[1].status, TodoStatus::Completed);
    assert_eq!(todos[2].content, "Run full suite");
    assert_eq!(todos[2].status, TodoStatus::InProgress);
}

#[test]
fn test_extract_plan_todos_handles_unknown_status_as_pending() {
    let content = r#"{"timestamp":"2026-04-02T15:24:40.322Z","type":"response_item","payload":{"type":"function_call","name":"update_plan","arguments":"{\"plan\":[{\"step\":\"Investigate bug\",\"status\":\"blocked-ish\"}]}","call_id":"call_1"}}"#;
    let events = parse_rollout(content);
    let todos = extract_plan_todos(&events);

    assert_eq!(todos.len(), 1);
    assert_eq!(todos[0].content, "Investigate bug");
    assert_eq!(todos[0].status, TodoStatus::Pending);
    assert_eq!(todos[0].priority, TodoPriority::Medium);
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
fn test_infer_rollout_status_aborted_beats_older_tool_call() {
    let content = r#"{"timestamp":"2026-03-31T10:00:00.000Z","type":"session_meta","payload":{"id":"test","cwd":"/tmp","cli_version":"0.1","source":"cli","model_provider":"openai","git":{"sha":"abc","branch":"main","origin_url":""}}}
{"timestamp":"2026-03-31T10:00:01.000Z","type":"response_item","payload":{"type":"function_call","name":"write_stdin","arguments":"{}","call_id":"call_1"}}
{"timestamp":"2026-03-31T10:00:02.000Z","type":"event_msg","payload":{"type":"turn_aborted","turn_id":"turn_1"}}"#;
    let events = parse_rollout(content);
    let status = infer_rollout_status(&events);
    assert_eq!(status, CodexInferredStatus::Aborted);
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

#[test]
fn test_build_codex_snapshot_does_not_bind_pid_for_multiple_top_level_threads_in_same_cwd() {
    let registry = AgentRegistry::new();
    registry.register(AgentTool::Codex, PathBuf::from("/tmp/project"), 44);

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
    assert!(snapshot
        .top_level_threads
        .iter()
        .all(|thread| thread.pid.is_none()));
}

#[test]
fn test_build_codex_snapshot_does_not_bind_pid_for_stale_unique_top_level_thread_without_rollout() {
    let registry = AgentRegistry::new();
    registry.register(AgentTool::Codex, PathBuf::from("/tmp/project"), 44);

    let threads = vec![CodexThread {
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
    }];

    let snapshot = build_codex_snapshot(&threads, &[], &registry);

    assert_eq!(snapshot.top_level_threads.len(), 1);
    assert_eq!(snapshot.top_level_threads[0].pid, None);
}

#[test]
fn test_build_codex_snapshot_binds_pid_when_rollout_is_available() {
    let registry = AgentRegistry::new();
    registry.register(AgentTool::Codex, PathBuf::from("/tmp/project"), 44);
    let now = chrono::Local::now().timestamp();

    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let rollout_path = std::env::temp_dir().join(format!("codex-pid-binding-{unique}.jsonl"));
    std::fs::write(
        &rollout_path,
        concat!(
            "{\"timestamp\":\"2026-03-31T10:00:00.000Z\",\"type\":\"session_meta\",\"payload\":{\"id\":\"thread-a\",\"cwd\":\"/tmp/project\",\"cli_version\":\"0.117.0\",\"source\":\"cli\",\"model_provider\":\"openai\",\"git\":{\"sha\":\"abc123\",\"branch\":\"main\",\"origin_url\":\"git@github.com:user/repo.git\"}}}\n",
            "{\"timestamp\":\"2026-03-31T10:00:01.000Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"task_started\",\"turn_id\":\"turn_1\"}}\n"
        ),
    )
    .unwrap();

    let threads = vec![CodexThread {
        id: "thread-a".into(),
        title: Some("A".into()),
        cwd: Some("/tmp/project".into()),
        source: Some("cli".into()),
        model: Some("gpt-5".into()),
        git_branch: Some("main".into()),
        agent_nickname: None,
        agent_role: None,
        created_at: now - 5,
        updated_at: now,
        archived: 0,
        tokens_used: None,
        rollout_path: Some(rollout_path.to_string_lossy().to_string()),
    }];

    let snapshot = build_codex_snapshot(&threads, &[], &registry);

    assert_eq!(snapshot.top_level_threads.len(), 1);
    assert_eq!(snapshot.top_level_threads[0].pid, Some(44));

    std::fs::remove_file(rollout_path).unwrap();
}

#[test]
fn test_build_codex_snapshot_binds_pid_from_exact_thread_mapping_even_when_stale() {
    let registry = AgentRegistry::new();
    registry.register(AgentTool::Codex, PathBuf::from("/tmp/project"), 44);
    registry.register_codex_thread("thread-a".to_string(), 15642);

    let threads = vec![CodexThread {
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
    }];

    let snapshot = build_codex_snapshot(&threads, &[], &registry);

    assert_eq!(snapshot.top_level_threads.len(), 1);
    assert_eq!(snapshot.top_level_threads[0].pid, Some(15642));
}

#[test]
fn test_build_codex_snapshot_ignores_stale_same_cwd_thread_for_pid_binding() {
    let registry = AgentRegistry::new();
    registry.register(AgentTool::Codex, PathBuf::from("/tmp/project"), 44);
    let now = chrono::Local::now().timestamp();

    let threads = vec![
        CodexThread {
            id: "fresh-thread".into(),
            title: Some("Fresh".into()),
            cwd: Some("/tmp/project".into()),
            source: Some("cli".into()),
            model: Some("gpt-5".into()),
            git_branch: Some("main".into()),
            agent_nickname: None,
            agent_role: None,
            created_at: now - 15,
            updated_at: now - 5,
            archived: 0,
            tokens_used: None,
            rollout_path: None,
        },
        CodexThread {
            id: "stale-thread".into(),
            title: Some("Stale".into()),
            cwd: Some("/tmp/project".into()),
            source: Some("cli".into()),
            model: Some("gpt-5".into()),
            git_branch: Some("main".into()),
            agent_nickname: None,
            agent_role: None,
            created_at: now - 3_600,
            updated_at: now - 3_000,
            archived: 0,
            tokens_used: None,
            rollout_path: None,
        },
    ];

    let snapshot = build_codex_snapshot(&threads, &[], &registry);
    let fresh = snapshot
        .top_level_threads
        .iter()
        .find(|thread| thread.thread.id == "fresh-thread")
        .unwrap();
    let stale = snapshot
        .top_level_threads
        .iter()
        .find(|thread| thread.thread.id == "stale-thread")
        .unwrap();

    assert_eq!(fresh.pid, Some(44));
    assert_eq!(stale.pid, None);
}

#[test]
fn test_build_codex_snapshot_binds_multiple_same_cwd_threads_by_launch_time() {
    let registry = AgentRegistry::new();
    let cwd = PathBuf::from("/tmp/project");
    let now = chrono::Local::now().timestamp();
    let first_start = now - 40;
    let second_start = now - 14;
    registry.register(AgentTool::Codex, cwd.clone(), 101);
    registry.register(AgentTool::Codex, cwd.clone(), 202);
    registry.register_codex_launch(101, from_epoch_secs(first_start));
    registry.register_codex_launch(202, from_epoch_secs(second_start));

    let threads = vec![
        CodexThread {
            id: "older-thread".into(),
            title: Some("Older".into()),
            cwd: Some("/tmp/project".into()),
            source: Some("cli".into()),
            model: Some("gpt-5".into()),
            git_branch: Some("main".into()),
            agent_nickname: None,
            agent_role: None,
            created_at: first_start + 2,
            updated_at: first_start + 12,
            archived: 0,
            tokens_used: None,
            rollout_path: None,
        },
        CodexThread {
            id: "newer-thread".into(),
            title: Some("Newer".into()),
            cwd: Some("/tmp/project".into()),
            source: Some("cli".into()),
            model: Some("gpt-5".into()),
            git_branch: Some("main".into()),
            agent_nickname: None,
            agent_role: None,
            created_at: second_start + 2,
            updated_at: second_start + 10,
            archived: 0,
            tokens_used: None,
            rollout_path: None,
        },
        CodexThread {
            id: "stale-thread".into(),
            title: Some("Stale".into()),
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
    ];

    let snapshot = build_codex_snapshot(&threads, &[], &registry);
    let older = snapshot
        .top_level_threads
        .iter()
        .find(|thread| thread.thread.id == "older-thread")
        .unwrap();
    let newer = snapshot
        .top_level_threads
        .iter()
        .find(|thread| thread.thread.id == "newer-thread")
        .unwrap();
    let stale = snapshot
        .top_level_threads
        .iter()
        .find(|thread| thread.thread.id == "stale-thread")
        .unwrap();

    assert_eq!(older.pid, Some(101));
    assert_eq!(newer.pid, Some(202));
    assert_eq!(stale.pid, None);
}

#[test]
fn test_build_codex_snapshot_does_not_guess_when_same_cwd_pid_count_is_ambiguous() {
    let registry = AgentRegistry::new();
    let cwd = PathBuf::from("/tmp/project");
    let now = chrono::Local::now().timestamp();
    let first_start = now - 50;
    let second_start = now - 30;

    registry.register(AgentTool::Codex, cwd.clone(), 101);
    registry.register(AgentTool::Codex, cwd.clone(), 202);
    registry.register_codex_thread("exact-thread".to_string(), 101);
    registry.register_codex_launch(202, from_epoch_secs(second_start));

    let threads = vec![
        CodexThread {
            id: "exact-thread".into(),
            title: Some("Exact".into()),
            cwd: Some("/tmp/project".into()),
            source: Some("cli".into()),
            model: Some("gpt-5".into()),
            git_branch: Some("main".into()),
            agent_nickname: None,
            agent_role: None,
            created_at: first_start + 2,
            updated_at: first_start + 8,
            archived: 0,
            tokens_used: None,
            rollout_path: None,
        },
        CodexThread {
            id: "candidate-a".into(),
            title: Some("Candidate A".into()),
            cwd: Some("/tmp/project".into()),
            source: Some("cli".into()),
            model: Some("gpt-5".into()),
            git_branch: Some("main".into()),
            agent_nickname: None,
            agent_role: None,
            created_at: second_start + 1,
            updated_at: second_start + 5,
            archived: 0,
            tokens_used: None,
            rollout_path: None,
        },
        CodexThread {
            id: "candidate-b".into(),
            title: Some("Candidate B".into()),
            cwd: Some("/tmp/project".into()),
            source: Some("cli".into()),
            model: Some("gpt-5".into()),
            git_branch: Some("main".into()),
            agent_nickname: None,
            agent_role: None,
            created_at: second_start + 3,
            updated_at: second_start + 9,
            archived: 0,
            tokens_used: None,
            rollout_path: None,
        },
    ];

    let snapshot = build_codex_snapshot(&threads, &[], &registry);
    let exact = snapshot
        .top_level_threads
        .iter()
        .find(|thread| thread.thread.id == "exact-thread")
        .unwrap();
    let candidate_a = snapshot
        .top_level_threads
        .iter()
        .find(|thread| thread.thread.id == "candidate-a")
        .unwrap();
    let candidate_b = snapshot
        .top_level_threads
        .iter()
        .find(|thread| thread.thread.id == "candidate-b")
        .unwrap();

    assert_eq!(exact.pid, Some(101));
    assert_eq!(candidate_a.pid, None);
    assert_eq!(candidate_b.pid, None);
}

#[test]
fn test_build_codex_event_snapshot_defaults_missing_rollout_to_waiting() {
    let now = chrono::Local::now().timestamp();
    let snapshot = CodexSnapshot {
        top_level_threads: vec![ObservedCodexThread {
            thread: CodexThread {
                id: "parent".into(),
                title: Some("Parent".into()),
                cwd: Some("/tmp/project".into()),
                source: Some("cli".into()),
                model: Some("gpt-5".into()),
                git_branch: Some("main".into()),
                agent_nickname: None,
                agent_role: None,
                created_at: now - 10,
                updated_at: now,
                archived: 0,
                tokens_used: None,
                rollout_path: None,
            },
            agent_id: AgentId::session(AgentTool::Codex, "parent"),
            pid: None,
            classification: CodexThreadClassification::TopLevel,
        }],
        child_threads: Vec::new(),
    };

    let event_snapshot = build_codex_event_snapshot(&snapshot, &[]);

    assert_eq!(event_snapshot.sessions.len(), 1);
    assert_eq!(event_snapshot.sessions[0].status, PrimaryStatus::Waiting);
}

#[test]
fn test_detect_codex_backend_missing_db_on_clean_start_is_not_degraded() {
    let registry = AgentRegistry::new();
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let db_path = std::env::temp_dir().join(format!("codex-missing-db-{unique}.sqlite"));

    let result = detect_codex_backend(&db_path, &registry);

    assert_eq!(result.backend_health, CodexBackendHealth::Healthy);
    assert!(result.snapshot.sessions.is_empty());
}

#[test]
fn test_detect_codex_backend_missing_db_stays_healthy_across_repeated_clean_start_polls() {
    let registry = AgentRegistry::new();
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let db_path = std::env::temp_dir().join(format!("codex-missing-db-repeat-{unique}.sqlite"));

    let first = detect_codex_backend_with_history(&db_path, &registry, false);
    let second = detect_codex_backend_with_history(&db_path, &registry, false);

    assert_eq!(first.backend_health, CodexBackendHealth::Healthy);
    assert_eq!(second.backend_health, CodexBackendHealth::Healthy);
    assert!(first.snapshot.sessions.is_empty());
    assert!(second.snapshot.sessions.is_empty());
}

#[test]
fn test_detect_codex_backend_reports_degraded_for_missing_db_after_previous_state() {
    let registry = AgentRegistry::new();
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let db_path = std::env::temp_dir().join(format!("codex-missing-db-late-{unique}.sqlite"));

    let result = detect_codex_backend_with_history(&db_path, &registry, true);

    assert!(matches!(
        result.backend_health,
        CodexBackendHealth::Degraded { ref reason } if reason.contains("missing")
    ));
    assert_eq!(result.snapshot.sessions.len(), 1);
    assert_eq!(result.snapshot.sessions[0].pid, Some(0));
    assert_eq!(
        result.snapshot.sessions[0].agent_id.session_id(),
        Some("codex-backend")
    );
    assert_eq!(
        result.snapshot.sessions[0].final_output.as_deref(),
        Some("Codex backend is degraded: sqlite state is missing")
    );
}

#[test]
fn test_detect_codex_backend_reports_degraded_for_threads_schema_drift() {
    let registry = AgentRegistry::new();
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let db_path = std::env::temp_dir().join(format!("codex-schema-drift-{unique}.sqlite"));
    let conn = Connection::open(&db_path).unwrap();
    conn.execute("CREATE TABLE not_threads (id INTEGER);", [])
        .unwrap();

    let result = detect_codex_backend(&db_path, &registry);

    assert!(matches!(
        result.backend_health,
        CodexBackendHealth::Degraded { ref reason } if reason.contains("threads")
    ));
    assert_eq!(result.snapshot.sessions.len(), 1);
    assert_eq!(result.snapshot.sessions[0].status, PrimaryStatus::Error);
}

#[test]
fn test_detect_codex_backend_reports_degraded_explicitly_when_threads_load_but_edges_fail() {
    let registry = AgentRegistry::new();
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let db_path = std::env::temp_dir().join(format!("codex-partial-drift-{unique}.sqlite"));
    let conn = Connection::open(&db_path).unwrap();
    conn.execute_batch(
        r#"
        CREATE TABLE threads (
            id TEXT PRIMARY KEY,
            title TEXT,
            cwd TEXT,
            source TEXT,
            model TEXT,
            git_branch TEXT,
            agent_nickname TEXT,
            agent_role TEXT,
            created_at INTEGER,
            updated_at INTEGER,
            archived INTEGER,
            tokens_used INTEGER,
            rollout_path TEXT
        );
        INSERT INTO threads (
            id, title, cwd, source, model, git_branch, agent_nickname, agent_role,
            created_at, updated_at, archived, tokens_used, rollout_path
        ) VALUES (
            'thread-a', 'Thread A', '/tmp/project', 'cli', 'gpt-5', 'main',
            NULL, NULL, 10, 20, 0, NULL, NULL
        );
        "#,
    )
    .unwrap();

    let result = detect_codex_backend(&db_path, &registry);

    assert!(matches!(
        result.backend_health,
        CodexBackendHealth::Degraded { ref reason } if reason.contains("spawn edges")
    ));
    assert_eq!(result.snapshot.sessions.len(), 2);
    assert_eq!(result.snapshot.sessions[0].pid, Some(0));
    assert_eq!(
        result.snapshot.sessions[0].agent_id.session_id(),
        Some("codex-backend")
    );
    assert!(result
        .snapshot
        .sessions
        .iter()
        .any(|session| session.agent_id.session_id() == Some("thread-a")));
    assert!(result
        .snapshot
        .sessions
        .iter()
        .any(|session| session.agent_id.session_id() == Some("codex-backend")
            && session.final_output.as_deref()
                == Some("Codex backend is degraded: sqlite spawn edges query failed: no such table: thread_spawn_edges")));
}

#[test]
fn test_reconcile_codex_snapshots_emits_backend_recovery_exit() {
    let previous = CodexEventSnapshot {
        sessions: vec![ObservedCodexSession {
            agent_id: AgentId::session(AgentTool::Codex, "codex-backend"),
            pid: Some(0),
            cwd: PathBuf::from("/"),
            branch: None,
            started_at: None,
            status: PrimaryStatus::Error,
            final_output: Some("Codex backend is degraded: sqlite state is missing".into()),
            final_output_is_completion: false,
            latest_tool_call: None,
            todos: Vec::new(),
            subagents: Vec::new(),
        }],
    };
    let current = CodexEventSnapshot {
        sessions: vec![ObservedCodexSession {
            agent_id: AgentId::session(AgentTool::Codex, "thread-a"),
            pid: Some(41),
            cwd: PathBuf::from("/tmp/project"),
            branch: Some("main".into()),
            started_at: from_epoch_secs(10),
            status: PrimaryStatus::Waiting,
            final_output: None,
            final_output_is_completion: false,
            latest_tool_call: None,
            todos: Vec::new(),
            subagents: Vec::new(),
        }],
    };

    let events = reconcile_codex_snapshots(Some(&previous), &current);

    assert!(events.iter().any(|event| matches!(
        event,
        AgentEvent::ProcessExited { id, pid }
            if id == &AgentId::session(AgentTool::Codex, "codex-backend")
                && pid == &Some(0)
    )));
}

#[test]
fn test_detect_codex_backend_healthy_snapshot_stays_healthy() {
    let registry = AgentRegistry::new();
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let db_path = std::env::temp_dir().join(format!("codex-healthy-{unique}.sqlite"));
    let conn = Connection::open(&db_path).unwrap();
    conn.execute_batch(
        r#"
        CREATE TABLE threads (
            id TEXT PRIMARY KEY,
            title TEXT,
            cwd TEXT,
            source TEXT,
            model TEXT,
            git_branch TEXT,
            agent_nickname TEXT,
            agent_role TEXT,
            created_at INTEGER,
            updated_at INTEGER,
            archived INTEGER,
            tokens_used INTEGER,
            rollout_path TEXT
        );
        CREATE TABLE thread_spawn_edges (
            parent_thread_id TEXT,
            child_thread_id TEXT,
            status TEXT
        );
        INSERT INTO threads (
            id, title, cwd, source, model, git_branch, agent_nickname, agent_role,
            created_at, updated_at, archived, tokens_used, rollout_path
        ) VALUES (
            'thread-a', 'Thread A', '/tmp/project', 'cli', 'gpt-5', 'main',
            NULL, NULL, 10, 20, 0, NULL, NULL
        );
        "#,
    )
    .unwrap();

    let result = detect_codex_backend(&db_path, &registry);

    assert_eq!(result.backend_health, CodexBackendHealth::Healthy);
    assert_eq!(result.snapshot.sessions.len(), 1);
    assert_eq!(result.snapshot.sessions[0].status, PrimaryStatus::Waiting);
    assert_eq!(
        result.snapshot.sessions[0].agent_id,
        AgentId::session(AgentTool::Codex, "thread-a")
    );
}

#[test]
fn test_codex_open_subagent_maps_to_working_subagent() {
    let session = ObservedCodexSession {
        subagents: vec![ObservedCodexSubagent {
            thread_id: "child-1".to_string(),
            name: "Darwin".to_string(),
            description: Some("child-1".to_string()),
            status: PrimaryStatus::Running,
            reason: None,

            started_at: None,
        }],
        ..observed_session("session-a", Some(41), PrimaryStatus::Running)
    };

    let normalized = normalize_codex_session(&session, Some("waiting on subagent"));
    assert_eq!(normalized.status, NormalizedStatus::Working);
    assert_eq!(normalized.reason, Some(NormalizedReason::Subagent));
}

#[test]
fn test_codex_done_review_output_beats_running_subagent() {
    let session = ObservedCodexSession {
        subagents: vec![ObservedCodexSubagent {
            thread_id: "child-1".to_string(),
            name: "Darwin".to_string(),
            description: Some("child-1".to_string()),
            status: PrimaryStatus::Running,
            reason: None,

            started_at: None,
        }],
        ..observed_session("session-a", Some(41), PrimaryStatus::Done)
    };

    let normalized = normalize_codex_session(&session, Some("### Issues\n#### Critical"));
    assert_eq!(normalized.status, NormalizedStatus::NeedsInput);
    assert_eq!(normalized.reason, Some(NormalizedReason::Review));
}

#[test]
fn test_codex_waiting_question_output_beats_running_subagent() {
    let session = ObservedCodexSession {
        subagents: vec![ObservedCodexSubagent {
            thread_id: "child-1".to_string(),
            name: "Darwin".to_string(),
            description: Some("child-1".to_string()),
            status: PrimaryStatus::Running,
            reason: None,

            started_at: None,
        }],
        ..observed_session("session-a", Some(41), PrimaryStatus::Waiting)
    };

    let normalized = normalize_codex_session(
        &session,
        Some("I can continue either way. Which option do you want me to take?"),
    );
    assert_eq!(normalized.status, NormalizedStatus::NeedsInput);
    assert_eq!(normalized.reason, Some(NormalizedReason::Decision));
}

#[test]
fn test_codex_waiting_subagent_approval_maps_parent_to_needs_input() {
    let session = ObservedCodexSession {
        subagents: vec![ObservedCodexSubagent {
            thread_id: "child-1".to_string(),
            name: "Darwin".to_string(),
            description: Some("child-1".to_string()),
            status: PrimaryStatus::Waiting,
            reason: Some(NormalizedReason::Approval),

            started_at: None,
        }],
        ..observed_session("session-a", Some(41), PrimaryStatus::Waiting)
    };

    let normalized = normalize_codex_session(&session, Some("waiting on subagent"));
    assert_eq!(normalized.status, NormalizedStatus::NeedsInput);
    assert_eq!(normalized.reason, Some(NormalizedReason::Approval));
}

#[test]
fn test_codex_waiting_subagent_decision_maps_parent_to_needs_input() {
    let session = ObservedCodexSession {
        subagents: vec![ObservedCodexSubagent {
            thread_id: "child-1".to_string(),
            name: "Darwin".to_string(),
            description: Some("child-1".to_string()),
            status: PrimaryStatus::Waiting,
            reason: Some(NormalizedReason::Decision),

            started_at: None,
        }],
        ..observed_session("session-a", Some(41), PrimaryStatus::Waiting)
    };

    let normalized = normalize_codex_session(&session, Some("waiting on subagent"));
    assert_eq!(normalized.status, NormalizedStatus::NeedsInput);
    assert_eq!(normalized.reason, Some(NormalizedReason::Decision));
}

#[test]
fn test_codex_running_tool_call_beats_running_subagent() {
    let session = ObservedCodexSession {
        latest_tool_call: Some(ObservedToolCall {
            name: "shell".to_string(),
            args_summary: Some("cargo test".to_string()),
        }),
        subagents: vec![ObservedCodexSubagent {
            thread_id: "child-1".to_string(),
            name: "Darwin".to_string(),
            description: Some("child-1".to_string()),
            status: PrimaryStatus::Running,
            reason: None,

            started_at: None,
        }],
        ..observed_session("session-a", Some(41), PrimaryStatus::Running)
    };

    let normalized = normalize_codex_session(&session, Some("Still working on it."));
    assert_eq!(normalized.status, NormalizedStatus::Working);
    assert_eq!(normalized.reason, Some(NormalizedReason::ToolCall));
}

#[test]
fn test_codex_review_completion_maps_to_needs_input_review() {
    let session = observed_session("session-a", Some(41), PrimaryStatus::Done);
    let normalized = normalize_codex_session(&session, Some("### Issues\n#### Critical"));
    assert_eq!(normalized.status, NormalizedStatus::NeedsInput);
    assert_eq!(normalized.reason, Some(NormalizedReason::Review));
}

#[test]
fn test_codex_assessment_completion_maps_to_needs_input_review() {
    let session = observed_session("session-a", Some(41), PrimaryStatus::Done);
    let normalized = normalize_codex_session(&session, Some("Merge-readiness assessment"));
    assert_eq!(normalized.status, NormalizedStatus::NeedsInput);
    assert_eq!(normalized.reason, Some(NormalizedReason::Review));
}

#[test]
fn test_codex_in_progress_review_like_output_stays_working() {
    let session = observed_session("session-a", Some(41), PrimaryStatus::Running);
    let normalized = normalize_codex_session(&session, Some("Review findings in progress"));
    assert_eq!(normalized.status, NormalizedStatus::Working);
    assert_eq!(normalized.reason, None);
}

#[test]
fn test_codex_done_without_final_output_maps_to_unknown() {
    let session = observed_session("session-a", Some(41), PrimaryStatus::Done);
    let normalized = normalize_codex_session(&session, None);
    assert_eq!(normalized.status, NormalizedStatus::Unknown);
    assert_eq!(normalized.reason, None);
}

#[test]
fn test_codex_waiting_without_rollout_evidence_maps_to_unknown() {
    let session = observed_session("session-a", Some(41), PrimaryStatus::Waiting);
    let normalized = normalize_codex_session(&session, None);
    assert_eq!(normalized.status, NormalizedStatus::Unknown);
    assert_eq!(normalized.reason, None);
}

#[test]
fn test_codex_completed_rollout_does_not_reuse_stale_assistant_text_as_final_output() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let rollout_path =
        std::env::temp_dir().join(format!("codex-stale-final-output-{unique}.jsonl"));
    std::fs::write(
        &rollout_path,
        concat!(
            "{\"timestamp\":\"2026-03-31T10:00:00.000Z\",\"type\":\"session_meta\",\"payload\":{\"id\":\"parent\",\"cwd\":\"/tmp/project\",\"cli_version\":\"0.117.0\",\"source\":\"cli\",\"model_provider\":\"openai\",\"git\":{\"sha\":\"abc123\",\"branch\":\"main\",\"origin_url\":\"git@github.com:user/repo.git\"}}}\n",
            "{\"timestamp\":\"2026-03-31T10:00:05.000Z\",\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"role\":\"assistant\",\"content\":[{\"type\":\"output_text\",\"text\":\"Still working through the changes.\"}]}}\n",
            "{\"timestamp\":\"2026-03-31T10:00:35.000Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"task_complete\",\"turn_id\":\"turn_1\",\"last_agent_message\":\"\"}}\n"
        ),
    )
    .unwrap();
    let now = chrono::Local::now().timestamp();
    let snapshot = CodexSnapshot {
        top_level_threads: vec![ObservedCodexThread {
            thread: CodexThread {
                id: "parent".into(),
                title: Some("Parent".into()),
                cwd: Some("/tmp/project".into()),
                source: Some("cli".into()),
                model: Some("gpt-5".into()),
                git_branch: Some("main".into()),
                agent_nickname: None,
                agent_role: None,
                created_at: now - 5,
                updated_at: now,
                archived: 0,
                tokens_used: None,
                rollout_path: Some(rollout_path.to_string_lossy().to_string()),
            },
            agent_id: AgentId::session(AgentTool::Codex, "parent"),
            pid: None,
            classification: CodexThreadClassification::TopLevel,
        }],
        child_threads: Vec::new(),
    };

    let event_snapshot = build_codex_event_snapshot(&snapshot, &[]);
    let normalized = normalize_codex_session(
        &event_snapshot.sessions[0],
        event_snapshot.sessions[0].final_output.as_deref(),
    );

    assert_eq!(event_snapshot.sessions[0].status, PrimaryStatus::Done);
    assert_eq!(event_snapshot.sessions[0].final_output, None);
    assert_eq!(normalized.status, NormalizedStatus::Unknown);

    std::fs::remove_file(rollout_path).unwrap();
}

#[test]
fn test_codex_new_tool_call_after_completion_revives_running_and_clears_stale_done_output() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let rollout_path =
        std::env::temp_dir().join(format!("codex-revived-after-complete-{unique}.jsonl"));
    std::fs::write(
        &rollout_path,
        concat!(
            "{\"timestamp\":\"2026-03-31T10:00:00.000Z\",\"type\":\"session_meta\",\"payload\":{\"id\":\"parent\",\"cwd\":\"/tmp/project\",\"cli_version\":\"0.117.0\",\"source\":\"cli\",\"model_provider\":\"openai\",\"git\":{\"sha\":\"abc123\",\"branch\":\"main\",\"origin_url\":\"git@github.com:user/repo.git\"}}}\n",
            "{\"timestamp\":\"2026-03-31T10:00:10.000Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"task_complete\",\"turn_id\":\"turn_1\",\"last_agent_message\":\"Ready for the next task.\"}}\n",
            "{\"timestamp\":\"2026-03-31T10:01:00.000Z\",\"type\":\"response_item\",\"payload\":{\"type\":\"function_call\",\"name\":\"shell\",\"arguments\":\"{\\\"cmd\\\":\\\"cargo test\\\"}\"}}\n"
        ),
    )
    .unwrap();

    let now = chrono::Local::now().timestamp();
    let snapshot = CodexSnapshot {
        top_level_threads: vec![ObservedCodexThread {
            thread: CodexThread {
                id: "parent".into(),
                title: Some("Parent".into()),
                cwd: Some("/tmp/project".into()),
                source: Some("cli".into()),
                model: Some("gpt-5".into()),
                git_branch: Some("main".into()),
                agent_nickname: None,
                agent_role: None,
                created_at: now - 5,
                updated_at: now,
                archived: 0,
                tokens_used: None,
                rollout_path: Some(rollout_path.to_string_lossy().to_string()),
            },
            agent_id: AgentId::session(AgentTool::Codex, "parent"),
            pid: Some(17254),
            classification: CodexThreadClassification::TopLevel,
        }],
        child_threads: Vec::new(),
    };

    let event_snapshot = build_codex_event_snapshot(&snapshot, &[]);
    let session = &event_snapshot.sessions[0];

    assert_eq!(session.status, PrimaryStatus::Running);
    assert_eq!(session.final_output, None);
    assert_eq!(
        session
            .latest_tool_call
            .as_ref()
            .map(|tool| tool.name.as_str()),
        Some("shell")
    );

    std::fs::remove_file(rollout_path).unwrap();
}

#[test]
fn test_live_pid_with_completed_rollout_stays_running_not_done() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let rollout_path =
        std::env::temp_dir().join(format!("codex-live-pid-completed-rollout-{unique}.jsonl"));
    std::fs::write(
        &rollout_path,
        concat!(
            "{\"timestamp\":\"2026-03-31T10:00:00.000Z\",\"type\":\"session_meta\",\"payload\":{\"id\":\"parent\",\"cwd\":\"/tmp/project\",\"cli_version\":\"0.117.0\",\"source\":\"cli\",\"model_provider\":\"openai\",\"git\":{\"sha\":\"abc123\",\"branch\":\"main\",\"origin_url\":\"git@github.com:user/repo.git\"}}}\n",
            "{\"timestamp\":\"2026-03-31T10:00:10.000Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"task_complete\",\"turn_id\":\"turn_1\",\"last_agent_message\":\"Ready for the next task.\"}}\n"
        ),
    )
    .unwrap();

    let now = chrono::Local::now().timestamp();
    let snapshot = CodexSnapshot {
        top_level_threads: vec![ObservedCodexThread {
            thread: CodexThread {
                id: "parent".into(),
                title: Some("Parent".into()),
                cwd: Some("/tmp/project".into()),
                source: Some("cli".into()),
                model: Some("gpt-5".into()),
                git_branch: Some("main".into()),
                agent_nickname: None,
                agent_role: None,
                created_at: now - 5,
                updated_at: now,
                archived: 0,
                tokens_used: None,
                rollout_path: Some(rollout_path.to_string_lossy().to_string()),
            },
            agent_id: AgentId::session(AgentTool::Codex, "parent"),
            pid: Some(17254),
            classification: CodexThreadClassification::TopLevel,
        }],
        child_threads: Vec::new(),
    };

    let event_snapshot = build_codex_event_snapshot(&snapshot, &[]);
    let session = &event_snapshot.sessions[0];
    let normalized = normalize_codex_session(session, session.final_output.as_deref());

    assert_eq!(session.status, PrimaryStatus::Running);
    assert_eq!(normalized.status, NormalizedStatus::Done);
    assert_eq!(normalized.reason, Some(NormalizedReason::ReadyForNextTask));

    std::fs::remove_file(rollout_path).unwrap();
}

#[test]
fn test_live_pid_completed_turn_clears_stale_todos_and_spawned_subagents() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let rollout_path =
        std::env::temp_dir().join(format!("codex-live-pid-clears-turn-state-{unique}.jsonl"));
    std::fs::write(
        &rollout_path,
        concat!(
            "{\"timestamp\":\"2026-03-31T10:00:00.000Z\",\"type\":\"session_meta\",\"payload\":{\"id\":\"parent\",\"cwd\":\"/tmp/project\",\"cli_version\":\"0.117.0\",\"source\":\"cli\",\"model_provider\":\"openai\",\"git\":{\"sha\":\"abc123\",\"branch\":\"main\",\"origin_url\":\"git@github.com:user/repo.git\"}}}\n",
            "{\"timestamp\":\"2026-03-31T10:00:02.000Z\",\"type\":\"response_item\",\"payload\":{\"type\":\"function_call\",\"name\":\"update_plan\",\"arguments\":\"{\\\"plan\\\":[{\\\"step\\\":\\\"Investigate\\\",\\\"status\\\":\\\"in_progress\\\"}]\",\"call_id\":\"call_plan\"}}\n",
            "{\"timestamp\":\"2026-03-31T10:00:03.000Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"collab_agent_spawn_end\",\"call_id\":\"call_1\",\"sender_thread_id\":\"parent\",\"new_thread_id\":\"child-rollout\",\"new_agent_nickname\":\"Parfit\",\"new_agent_role\":\"worker\",\"prompt\":\"Implement the validation logic\"}}\n",
            "{\"timestamp\":\"2026-03-31T10:00:10.000Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"task_complete\",\"turn_id\":\"turn_1\",\"last_agent_message\":\"Ready for the next task.\"}}\n"
        ),
    )
    .unwrap();

    let now = chrono::Local::now().timestamp();
    let snapshot = CodexSnapshot {
        top_level_threads: vec![ObservedCodexThread {
            thread: CodexThread {
                id: "parent".into(),
                title: Some("Parent".into()),
                cwd: Some("/tmp/project".into()),
                source: Some("cli".into()),
                model: Some("gpt-5".into()),
                git_branch: Some("main".into()),
                agent_nickname: None,
                agent_role: None,
                created_at: now - 5,
                updated_at: now,
                archived: 0,
                tokens_used: None,
                rollout_path: Some(rollout_path.to_string_lossy().to_string()),
            },
            agent_id: AgentId::session(AgentTool::Codex, "parent"),
            pid: Some(17254),
            classification: CodexThreadClassification::TopLevel,
        }],
        child_threads: Vec::new(),
    };

    let event_snapshot = build_codex_event_snapshot(&snapshot, &[]);
    let session = &event_snapshot.sessions[0];

    assert!(session.todos.is_empty());
    assert!(session.subagents.is_empty());

    std::fs::remove_file(rollout_path).unwrap();
}

#[test]
fn test_live_pid_completed_turn_clears_stale_latest_tool_call() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let rollout_path =
        std::env::temp_dir().join(format!("codex-live-pid-clears-latest-tool-{unique}.jsonl"));
    std::fs::write(
        &rollout_path,
        concat!(
            "{\"timestamp\":\"2026-03-31T10:00:00.000Z\",\"type\":\"session_meta\",\"payload\":{\"id\":\"parent\",\"cwd\":\"/tmp/project\",\"cli_version\":\"0.117.0\",\"source\":\"cli\",\"model_provider\":\"openai\",\"git\":{\"sha\":\"abc123\",\"branch\":\"main\",\"origin_url\":\"git@github.com:user/repo.git\"}}}\n",
            "{\"timestamp\":\"2026-03-31T10:00:02.000Z\",\"type\":\"response_item\",\"payload\":{\"type\":\"function_call\",\"name\":\"exec_command\",\"arguments\":\"{\\\"cmd\\\":\\\"cargo test\\\"}\",\"call_id\":\"call_exec\"}}\n",
            "{\"timestamp\":\"2026-03-31T10:00:10.000Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"task_complete\",\"turn_id\":\"turn_1\",\"last_agent_message\":\"Ready for the next task.\"}}\n"
        ),
    )
    .unwrap();

    let now = chrono::Local::now().timestamp();
    let snapshot = CodexSnapshot {
        top_level_threads: vec![ObservedCodexThread {
            thread: CodexThread {
                id: "parent".into(),
                title: Some("Parent".into()),
                cwd: Some("/tmp/project".into()),
                source: Some("cli".into()),
                model: Some("gpt-5".into()),
                git_branch: Some("main".into()),
                agent_nickname: None,
                agent_role: None,
                created_at: now - 5,
                updated_at: now,
                archived: 0,
                tokens_used: None,
                rollout_path: Some(rollout_path.to_string_lossy().to_string()),
            },
            agent_id: AgentId::session(AgentTool::Codex, "parent"),
            pid: Some(17254),
            classification: CodexThreadClassification::TopLevel,
        }],
        child_threads: Vec::new(),
    };

    let event_snapshot = build_codex_event_snapshot(&snapshot, &[]);
    let session = &event_snapshot.sessions[0];

    assert!(session.latest_tool_call.is_none());

    std::fs::remove_file(rollout_path).unwrap();
}

#[test]
fn test_reconcile_live_completed_session_does_not_emit_generating_activity() {
    let previous = CodexEventSnapshot {
        sessions: vec![ObservedCodexSession {
            final_output: Some("Ready for the next task.".to_string()),
            ..observed_session("session-a", Some(41), PrimaryStatus::Done)
        }],
    };
    let current = CodexEventSnapshot {
        sessions: vec![ObservedCodexSession {
            final_output: Some("Ready for the next task.".to_string()),
            ..observed_session("session-a", Some(41), PrimaryStatus::Running)
        }],
    };

    let events = reconcile_codex_snapshots(Some(&previous), &current);

    assert!(events.iter().all(|event| {
        !matches!(
            event,
            AgentEvent::ActivityChanged {
                id,
                activity: Some(Activity::Generating),
                ..
            } if id == &AgentId::session(AgentTool::Codex, "session-a")
        )
    }));
}

#[test]
fn test_codex_completed_non_review_output_is_not_misclassified_as_review() {
    let session = observed_session("session-a", Some(41), PrimaryStatus::Done);
    let normalized = normalize_codex_session(
        &session,
        Some("Completed implementation after an assessment run."),
    );
    assert_eq!(normalized.status, NormalizedStatus::Done);
    assert_eq!(normalized.reason, Some(NormalizedReason::ReadyForNextTask));
}

#[test]
fn test_codex_live_completed_non_review_output_maps_to_ready() {
    let session = ObservedCodexSession {
        final_output_is_completion: true,
        ..observed_session("session-a", Some(41), PrimaryStatus::Running)
    };
    let normalized = normalize_codex_session(
        &session,
        Some("The root cause was the same-cwd fallback binding, not the status normalization."),
    );
    assert_eq!(normalized.status, NormalizedStatus::Done);
    assert_eq!(normalized.reason, Some(NormalizedReason::ReadyForNextTask));
}

#[test]
fn test_codex_live_nonfinal_assistant_output_stays_working() {
    let session = observed_session("session-a", Some(41), PrimaryStatus::Running);
    let normalized = normalize_codex_session(
        &session,
        Some("I found the exact thread id for PID 42132, so this is not another binding problem."),
    );
    assert_eq!(normalized.status, NormalizedStatus::Working);
    assert_eq!(normalized.reason, None);
}

#[test]
fn test_codex_requested_changes_phrase_inside_completion_message_stays_done() {
    let session = observed_session("session-a", Some(41), PrimaryStatus::Done);
    let normalized = normalize_codex_session(
        &session,
        Some("Implemented the requested changes and tests pass."),
    );
    assert_eq!(normalized.status, NormalizedStatus::Done);
    assert_eq!(normalized.reason, Some(NormalizedReason::ReadyForNextTask));
}

#[test]
fn test_reconcile_codex_snapshots_emits_session_keyed_top_level_events() {
    let current = CodexEventSnapshot {
        sessions: vec![ObservedCodexSession {
            latest_tool_call: Some(ObservedToolCall {
                name: "shell".to_string(),
                args_summary: Some("{\"cmd\":\"cargo test\"}".to_string()),
            }),
            subagents: vec![ObservedCodexSubagent {
                thread_id: "child-a".to_string(),
                name: "Nash".to_string(),
                description: Some("child-a".to_string()),
                status: PrimaryStatus::Running,
                reason: None,

                started_at: None,
            }],
            ..observed_session("session-a", Some(41), PrimaryStatus::Done)
        }],
    };

    let events = reconcile_codex_snapshots(None, &current);

    assert!(matches!(
        &events[0],
        AgentEvent::Discovered { id, pid, cwd, .. }
            if id == &AgentId::session(AgentTool::Codex, "session-a")
                && pid == &Some(41)
                && cwd == &PathBuf::from("/tmp/project")
    ));
    assert!(matches!(
        &events[1],
        AgentEvent::SessionInfo { id, pid, session_id, branch, .. }
            if id == &AgentId::session(AgentTool::Codex, "session-a")
                && pid == &Some(41)
                && session_id == "session-a"
                && branch.as_deref() == Some("main")
    ));
    assert!(matches!(
        &events[2],
        AgentEvent::StatusChanged { id, old, new, .. }
            if id == &AgentId::session(AgentTool::Codex, "session-a")
                && old == &PrimaryStatus::Running
                && new == &PrimaryStatus::Done
    ));
    assert!(matches!(
        &events[3],
        AgentEvent::NormalizedStateChanged { id, state, .. }
            if id == &AgentId::session(AgentTool::Codex, "session-a")
                && state.status == NormalizedStatus::Working
                && state.reason == Some(NormalizedReason::Subagent)
    ));
    assert!(events.iter().any(|event| {
        matches!(
            event,
            AgentEvent::ToolCallStarted { id, tool_name, .. }
                if id == &AgentId::session(AgentTool::Codex, "session-a")
                    && tool_name == "shell"
        )
    }));
    assert!(events.iter().any(|event| {
        matches!(
            event,
            AgentEvent::SubagentSpawned { id, subagent, .. }
                if id == &AgentId::session(AgentTool::Codex, "session-a")
                    && subagent.name == "Nash"
                    && subagent.status == PrimaryStatus::Running
        )
    }));
}

#[test]
fn test_reconcile_codex_snapshots_emits_tool_completion_and_restart() {
    let previous = CodexEventSnapshot {
        sessions: vec![ObservedCodexSession {
            latest_tool_call: Some(ObservedToolCall {
                name: "shell".to_string(),
                args_summary: Some("ls".to_string()),
            }),
            ..observed_session("session-a", Some(41), PrimaryStatus::Running)
        }],
    };
    let current = CodexEventSnapshot {
        sessions: vec![ObservedCodexSession {
            latest_tool_call: Some(ObservedToolCall {
                name: "apply_patch".to_string(),
                args_summary: Some("src/detect/codex.rs".to_string()),
            }),
            ..observed_session("session-a", Some(41), PrimaryStatus::Running)
        }],
    };

    let events = reconcile_codex_snapshots(Some(&previous), &current);

    assert!(matches!(
        &events[0],
        AgentEvent::ToolCallCompleted { id, .. }
            if id == &AgentId::session(AgentTool::Codex, "session-a")
    ));
    assert!(matches!(
        &events[1],
        AgentEvent::ToolCallStarted { id, tool_name, .. }
            if id == &AgentId::session(AgentTool::Codex, "session-a")
                && tool_name == "apply_patch"
    ));
}

#[test]
fn test_reconcile_codex_snapshots_emits_todos_updated_for_plan_changes() {
    let previous = CodexEventSnapshot {
        sessions: vec![ObservedCodexSession {
            todos: vec![TodoItem {
                content: "Read files".to_string(),
                status: TodoStatus::InProgress,
                priority: TodoPriority::Medium,
            }],
            ..observed_session("session-a", Some(41), PrimaryStatus::Running)
        }],
    };
    let current = CodexEventSnapshot {
        sessions: vec![ObservedCodexSession {
            todos: vec![
                TodoItem {
                    content: "Read files".to_string(),
                    status: TodoStatus::Completed,
                    priority: TodoPriority::Medium,
                },
                TodoItem {
                    content: "Run tests".to_string(),
                    status: TodoStatus::InProgress,
                    priority: TodoPriority::Medium,
                },
            ],
            ..observed_session("session-a", Some(41), PrimaryStatus::Running)
        }],
    };

    let events = reconcile_codex_snapshots(Some(&previous), &current);

    assert!(events.iter().any(|event| {
        matches!(
            event,
            AgentEvent::TodosUpdated { id, todos, .. }
                if id == &AgentId::session(AgentTool::Codex, "session-a")
                    && todos.len() == 2
                    && todos[0].status == TodoStatus::Completed
                    && todos[1].content == "Run tests"
        )
    }));
}

#[test]
fn test_reconcile_codex_snapshots_emits_child_completion_from_snapshot_delta() {
    let previous = CodexEventSnapshot {
        sessions: vec![ObservedCodexSession {
            subagents: vec![ObservedCodexSubagent {
                thread_id: "child-a".to_string(),
                name: "Nash".to_string(),
                description: Some("child-a".to_string()),
                status: PrimaryStatus::Running,
                reason: None,

                started_at: None,
            }],
            ..observed_session("session-a", Some(41), PrimaryStatus::Running)
        }],
    };
    let current = CodexEventSnapshot {
        sessions: vec![ObservedCodexSession {
            subagents: vec![ObservedCodexSubagent {
                thread_id: "child-a".to_string(),
                name: "Nash".to_string(),
                description: Some("child-a".to_string()),
                status: PrimaryStatus::Done,
                reason: None,

                started_at: None,
            }],
            ..observed_session("session-a", Some(41), PrimaryStatus::Running)
        }],
    };

    let events = reconcile_codex_snapshots(Some(&previous), &current);

    assert!(matches!(
        events.iter().find(|e| matches!(e, AgentEvent::NormalizedStateChanged { .. })),
        Some(AgentEvent::NormalizedStateChanged { id, state, .. })
            if id == &AgentId::session(AgentTool::Codex, "session-a")
                && state.status == NormalizedStatus::Working
                && state.reason.is_none()
    ));
    assert!(events.iter().any(|e| matches!(
        e,
        AgentEvent::ActivityChanged { id, .. }
            if id == &AgentId::session(AgentTool::Codex, "session-a")
    )));
    assert!(events.iter().any(|e| matches!(
        e,
        AgentEvent::SubagentCompleted { id, subagent_name, .. }
            if id == &AgentId::session(AgentTool::Codex, "session-a")
                && subagent_name == "child-a"
    )));
}

#[test]
fn test_reconcile_codex_snapshots_completes_duplicate_nickname_child_by_thread_id() {
    let previous = CodexEventSnapshot {
        sessions: vec![ObservedCodexSession {
            subagents: vec![
                ObservedCodexSubagent {
                    thread_id: "child-a".to_string(),
                    name: "Nash".to_string(),
                    description: Some("child-a".to_string()),
                    status: PrimaryStatus::Running,
                    reason: None,

                    started_at: None,
                },
                ObservedCodexSubagent {
                    thread_id: "child-b".to_string(),
                    name: "Nash".to_string(),
                    description: Some("child-b".to_string()),
                    status: PrimaryStatus::Running,
                    reason: None,

                    started_at: None,
                },
            ],
            ..observed_session("session-a", Some(41), PrimaryStatus::Running)
        }],
    };
    let current = CodexEventSnapshot {
        sessions: vec![ObservedCodexSession {
            subagents: vec![
                ObservedCodexSubagent {
                    thread_id: "child-a".to_string(),
                    name: "Nash".to_string(),
                    description: Some("child-a".to_string()),
                    status: PrimaryStatus::Running,
                    reason: None,

                    started_at: None,
                },
                ObservedCodexSubagent {
                    thread_id: "child-b".to_string(),
                    name: "Nash".to_string(),
                    description: Some("child-b".to_string()),
                    status: PrimaryStatus::Done,
                    reason: None,

                    started_at: None,
                },
            ],
            ..observed_session("session-a", Some(41), PrimaryStatus::Running)
        }],
    };

    let events = reconcile_codex_snapshots(Some(&previous), &current);

    assert_eq!(events.len(), 2);
    assert!(events.iter().any(|event| matches!(
        event,
        AgentEvent::SubagentsUpdated { id, subagents, .. }
            if id == &AgentId::session(AgentTool::Codex, "session-a")
                && subagents.len() == 2
    )));
    assert!(events.iter().any(|event| matches!(
        event,
        AgentEvent::SubagentCompleted { subagent_name, .. }
            if subagent_name == "child-b"
    )));
}

#[test]
fn test_reconcile_codex_snapshots_does_not_retire_missing_bound_session() {
    let previous = CodexEventSnapshot {
        sessions: vec![observed_session(
            "session-a",
            Some(41),
            PrimaryStatus::Running,
        )],
    };
    let current = CodexEventSnapshot { sessions: vec![] };

    let events = reconcile_codex_snapshots(Some(&previous), &current);

    assert!(events.is_empty());
}

#[test]
fn test_reconcile_codex_snapshots_does_not_retire_missing_unbound_session() {
    let previous = CodexEventSnapshot {
        sessions: vec![observed_session("session-a", None, PrimaryStatus::Running)],
    };
    let current = CodexEventSnapshot { sessions: vec![] };

    let events = reconcile_codex_snapshots(Some(&previous), &current);

    assert!(events.is_empty());
}

#[test]
fn test_build_codex_event_snapshot_keeps_top_level_session_when_cwd_missing() {
    let snapshot = CodexSnapshot {
        top_level_threads: vec![ObservedCodexThread {
            thread: CodexThread {
                id: "parent".into(),
                title: Some("Parent".into()),
                cwd: None,
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
            agent_id: AgentId::session(AgentTool::Codex, "parent"),
            pid: None,
            classification: CodexThreadClassification::TopLevel,
        }],
        child_threads: Vec::new(),
    };

    let event_snapshot = build_codex_event_snapshot(&snapshot, &[]);

    assert_eq!(event_snapshot.sessions.len(), 1);
    assert_eq!(
        event_snapshot.sessions[0].agent_id,
        AgentId::session(AgentTool::Codex, "parent")
    );
    assert_eq!(event_snapshot.sessions[0].cwd, PathBuf::new());
}

#[test]
fn test_build_codex_event_snapshot_includes_rollout_only_subagents() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let rollout_path = std::env::temp_dir().join(format!("codex-rollout-only-{unique}.jsonl"));
    std::fs::write(
        &rollout_path,
        concat!(
            "{\"timestamp\":\"2026-03-31T10:00:00.000Z\",\"type\":\"session_meta\",\"payload\":{\"id\":\"parent\",\"cwd\":\"/tmp/project\",\"cli_version\":\"0.117.0\",\"source\":\"cli\",\"model_provider\":\"openai\",\"git\":{\"sha\":\"abc123\",\"branch\":\"main\",\"origin_url\":\"git@github.com:user/repo.git\"}}}\n",
            "{\"timestamp\":\"2026-03-31T10:00:20.000Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"collab_agent_spawn_end\",\"call_id\":\"call_1\",\"sender_thread_id\":\"parent\",\"new_thread_id\":\"child-rollout\",\"new_agent_nickname\":\"Parfit\",\"new_agent_role\":\"worker\",\"prompt\":\"Implement the validation logic\"}}\n"
        ),
    )
    .unwrap();

    let snapshot = CodexSnapshot {
        top_level_threads: vec![ObservedCodexThread {
            thread: CodexThread {
                id: "parent".into(),
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
                rollout_path: Some(rollout_path.to_string_lossy().to_string()),
            },
            agent_id: AgentId::session(AgentTool::Codex, "parent"),
            pid: Some(41),
            classification: CodexThreadClassification::TopLevel,
        }],
        child_threads: Vec::new(),
    };

    let event_snapshot = build_codex_event_snapshot(&snapshot, &[]);

    assert_eq!(event_snapshot.sessions.len(), 1);
    assert_eq!(event_snapshot.sessions[0].subagents.len(), 1);
    assert_eq!(
        event_snapshot.sessions[0].subagents[0].thread_id,
        "child-rollout"
    );
    assert_eq!(event_snapshot.sessions[0].subagents[0].name, "Parfit");
    assert_eq!(
        event_snapshot.sessions[0].subagents[0]
            .description
            .as_deref(),
        Some("child-rollout")
    );

    std::fs::remove_file(rollout_path).unwrap();
}

#[test]
fn test_codex_open_child_without_rollout_keeps_live_parent_running() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let rollout_path =
        std::env::temp_dir().join(format!("codex-parent-complete-open-child-{unique}.jsonl"));
    std::fs::write(
        &rollout_path,
        concat!(
            "{\"timestamp\":\"2026-03-31T10:00:00.000Z\",\"type\":\"session_meta\",\"payload\":{\"id\":\"parent\",\"cwd\":\"/tmp/project\",\"cli_version\":\"0.117.0\",\"source\":\"cli\",\"model_provider\":\"openai\",\"git\":{\"sha\":\"abc123\",\"branch\":\"main\",\"origin_url\":\"git@github.com:user/repo.git\"}}}\n",
            "{\"timestamp\":\"2026-03-31T10:00:35.000Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"task_complete\",\"turn_id\":\"turn_1\",\"last_agent_message\":\"Ready for the next task.\"}}\n"
        ),
    )
    .unwrap();

    let snapshot = CodexSnapshot {
        top_level_threads: vec![ObservedCodexThread {
            thread: CodexThread {
                id: "parent".into(),
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
                rollout_path: Some(rollout_path.to_string_lossy().to_string()),
            },
            agent_id: AgentId::session(AgentTool::Codex, "parent"),
            pid: Some(41),
            classification: CodexThreadClassification::TopLevel,
        }],
        child_threads: vec![ObservedCodexThread {
            thread: CodexThread {
                id: "child-open".into(),
                title: Some("Child".into()),
                cwd: Some("/tmp/project".into()),
                source: Some(
                    "{\"subagent\":{\"thread_spawn\":{\"parent_thread_id\":\"parent\"}}}".into(),
                ),
                model: Some("gpt-5".into()),
                git_branch: Some("main".into()),
                agent_nickname: Some("Parfit".into()),
                agent_role: Some("worker".into()),
                created_at: 11,
                updated_at: 21,
                archived: 0,
                tokens_used: None,
                rollout_path: None,
            },
            agent_id: AgentId::session(AgentTool::Codex, "child-open"),
            pid: None,
            classification: CodexThreadClassification::Child {
                parent_thread_id: Some("parent".into()),
            },
        }],
    };
    let spawn_edges = vec![SpawnEdge {
        parent_thread_id: "parent".into(),
        child_thread_id: "child-open".into(),
        status: "open".into(),
    }];

    let event_snapshot = build_codex_event_snapshot(&snapshot, &spawn_edges);
    let session = &event_snapshot.sessions[0];
    let normalized = normalize_codex_session(session, session.final_output.as_deref());

    assert!(session.subagents.is_empty());
    assert_eq!(normalized.status, NormalizedStatus::Done);
    assert_eq!(normalized.reason, Some(NormalizedReason::ReadyForNextTask));

    std::fs::remove_file(rollout_path).unwrap();
}

#[test]
fn test_codex_rollout_only_spawn_keeps_live_parent_running() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let rollout_path =
        std::env::temp_dir().join(format!("codex-rollout-only-done-parent-{unique}.jsonl"));
    std::fs::write(
        &rollout_path,
        concat!(
            "{\"timestamp\":\"2026-03-31T10:00:00.000Z\",\"type\":\"session_meta\",\"payload\":{\"id\":\"parent\",\"cwd\":\"/tmp/project\",\"cli_version\":\"0.117.0\",\"source\":\"cli\",\"model_provider\":\"openai\",\"git\":{\"sha\":\"abc123\",\"branch\":\"main\",\"origin_url\":\"git@github.com:user/repo.git\"}}}\n",
            "{\"timestamp\":\"2026-03-31T10:00:20.000Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"collab_agent_spawn_end\",\"call_id\":\"call_1\",\"sender_thread_id\":\"parent\",\"new_thread_id\":\"child-rollout\",\"new_agent_nickname\":\"Parfit\",\"new_agent_role\":\"worker\",\"prompt\":\"Implement the validation logic\"}}\n",
            "{\"timestamp\":\"2026-03-31T10:00:35.000Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"task_complete\",\"turn_id\":\"turn_1\",\"last_agent_message\":\"Ready for the next task.\"}}\n"
        ),
    )
    .unwrap();

    let snapshot = CodexSnapshot {
        top_level_threads: vec![ObservedCodexThread {
            thread: CodexThread {
                id: "parent".into(),
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
                rollout_path: Some(rollout_path.to_string_lossy().to_string()),
            },
            agent_id: AgentId::session(AgentTool::Codex, "parent"),
            pid: Some(41),
            classification: CodexThreadClassification::TopLevel,
        }],
        child_threads: Vec::new(),
    };

    let event_snapshot = build_codex_event_snapshot(&snapshot, &[]);
    let session = &event_snapshot.sessions[0];
    let normalized = normalize_codex_session(session, session.final_output.as_deref());

    assert!(session.subagents.is_empty());
    assert_eq!(normalized.status, NormalizedStatus::Done);
    assert_eq!(normalized.reason, Some(NormalizedReason::ReadyForNextTask));

    std::fs::remove_file(rollout_path).unwrap();
}

#[test]
fn test_child_waiting_for_approval_bubbles_to_parent_needs_input() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let child_rollout = std::env::temp_dir().join(format!("codex-child-approval-{unique}.jsonl"));
    std::fs::write(
        &child_rollout,
        concat!(
            "{\"timestamp\":\"2026-03-31T10:00:00.000Z\",\"type\":\"session_meta\",\"payload\":{\"id\":\"child\",\"cwd\":\"/tmp/project\",\"cli_version\":\"0.117.0\",\"source\":\"cli\",\"model_provider\":\"openai\",\"git\":{\"sha\":\"abc123\",\"branch\":\"main\",\"origin_url\":\"git@github.com:user/repo.git\"}}}\n",
            "{\"timestamp\":\"2026-03-31T10:00:10.000Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"turn_aborted\",\"turn_id\":\"turn_1\"}}\n",
            "{\"timestamp\":\"2026-03-31T10:00:11.000Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"task_complete\",\"turn_id\":\"turn_1\",\"last_agent_message\":\"Need your approval before I continue.\"}}\n"
        ),
    )
    .unwrap();

    let snapshot = CodexSnapshot {
        top_level_threads: vec![ObservedCodexThread {
            thread: CodexThread {
                id: "parent".into(),
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
            agent_id: AgentId::session(AgentTool::Codex, "parent"),
            pid: Some(41),
            classification: CodexThreadClassification::TopLevel,
        }],
        child_threads: vec![ObservedCodexThread {
            thread: CodexThread {
                id: "child".into(),
                title: Some("Child".into()),
                cwd: Some("/tmp/project".into()),
                source: Some(
                    "{\"subagent\":{\"thread_spawn\":{\"parent_thread_id\":\"parent\"}}}".into(),
                ),
                model: Some("gpt-5".into()),
                git_branch: Some("main".into()),
                agent_nickname: Some("Euclid".into()),
                agent_role: Some("worker".into()),
                created_at: 11,
                updated_at: 21,
                archived: 0,
                tokens_used: None,
                rollout_path: Some(child_rollout.to_string_lossy().to_string()),
            },
            agent_id: AgentId::session(AgentTool::Codex, "child"),
            pid: None,
            classification: CodexThreadClassification::Child {
                parent_thread_id: Some("parent".into()),
            },
        }],
    };
    let spawn_edges = vec![SpawnEdge {
        parent_thread_id: "parent".into(),
        child_thread_id: "child".into(),
        status: "open".into(),
    }];

    let event_snapshot = build_codex_event_snapshot(&snapshot, &spawn_edges);
    let session = &event_snapshot.sessions[0];
    let normalized = normalize_codex_session(session, session.final_output.as_deref());

    assert_eq!(session.subagents.len(), 1);
    assert_eq!(session.subagents[0].status, PrimaryStatus::Done);
    assert_eq!(
        session.subagents[0].reason,
        Some(NormalizedReason::Approval)
    );
    assert_eq!(normalized.status, NormalizedStatus::NeedsInput);
    assert_eq!(normalized.reason, Some(NormalizedReason::Approval));

    std::fs::remove_file(child_rollout).unwrap();
}

#[test]
fn test_closed_aborted_child_maps_to_done_subagent() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let child_rollout =
        std::env::temp_dir().join(format!("codex-closed-aborted-child-{unique}.jsonl"));
    std::fs::write(
        &child_rollout,
        concat!(
            "{\"timestamp\":\"2026-03-31T10:00:00.000Z\",\"type\":\"session_meta\",\"payload\":{\"id\":\"child\",\"cwd\":\"/tmp/project\",\"cli_version\":\"0.117.0\",\"source\":\"cli\",\"model_provider\":\"openai\",\"git\":{\"sha\":\"abc123\",\"branch\":\"main\",\"origin_url\":\"git@github.com:user/repo.git\"}}}\n",
            "{\"timestamp\":\"2026-03-31T10:00:10.000Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"turn_aborted\",\"turn_id\":\"turn_1\"}}\n"
        ),
    )
    .unwrap();

    let snapshot = CodexSnapshot {
        top_level_threads: vec![ObservedCodexThread {
            thread: CodexThread {
                id: "parent".into(),
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
            agent_id: AgentId::session(AgentTool::Codex, "parent"),
            pid: Some(41),
            classification: CodexThreadClassification::TopLevel,
        }],
        child_threads: vec![ObservedCodexThread {
            thread: CodexThread {
                id: "child".into(),
                title: Some("Child".into()),
                cwd: Some("/tmp/project".into()),
                source: Some(
                    "{\"subagent\":{\"thread_spawn\":{\"parent_thread_id\":\"parent\"}}}".into(),
                ),
                model: Some("gpt-5".into()),
                git_branch: Some("main".into()),
                agent_nickname: Some("Euclid".into()),
                agent_role: Some("worker".into()),
                created_at: 11,
                updated_at: 21,
                archived: 0,
                tokens_used: None,
                rollout_path: Some(child_rollout.to_string_lossy().to_string()),
            },
            agent_id: AgentId::session(AgentTool::Codex, "child"),
            pid: None,
            classification: CodexThreadClassification::Child {
                parent_thread_id: Some("parent".into()),
            },
        }],
    };
    let spawn_edges = vec![SpawnEdge {
        parent_thread_id: "parent".into(),
        child_thread_id: "child".into(),
        status: "closed".into(),
    }];

    let event_snapshot = build_codex_event_snapshot(&snapshot, &spawn_edges);

    assert_eq!(event_snapshot.sessions.len(), 1);
    assert_eq!(event_snapshot.sessions[0].subagents.len(), 1);
    assert_eq!(event_snapshot.sessions[0].subagents[0].name, "Euclid");
    assert_eq!(
        event_snapshot.sessions[0].subagents[0].status,
        PrimaryStatus::Done
    );

    std::fs::remove_file(child_rollout).unwrap();
}

#[test]
fn test_closed_aborted_child_with_prior_tool_call_maps_to_done_subagent() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let child_rollout =
        std::env::temp_dir().join(format!("codex-closed-aborted-tool-child-{unique}.jsonl"));
    std::fs::write(
        &child_rollout,
        concat!(
            "{\"timestamp\":\"2026-03-31T10:00:00.000Z\",\"type\":\"session_meta\",\"payload\":{\"id\":\"child\",\"cwd\":\"/tmp/project\",\"cli_version\":\"0.117.0\",\"source\":\"cli\",\"model_provider\":\"openai\",\"git\":{\"sha\":\"abc123\",\"branch\":\"main\",\"origin_url\":\"git@github.com:user/repo.git\"}}}\n",
            "{\"timestamp\":\"2026-03-31T10:00:05.000Z\",\"type\":\"response_item\",\"payload\":{\"type\":\"function_call\",\"name\":\"write_stdin\",\"arguments\":\"{}\",\"call_id\":\"call_1\"}}\n",
            "{\"timestamp\":\"2026-03-31T10:00:10.000Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"turn_aborted\",\"turn_id\":\"turn_1\"}}\n"
        ),
    )
    .unwrap();

    let snapshot = CodexSnapshot {
        top_level_threads: vec![ObservedCodexThread {
            thread: CodexThread {
                id: "parent".into(),
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
            agent_id: AgentId::session(AgentTool::Codex, "parent"),
            pid: Some(41),
            classification: CodexThreadClassification::TopLevel,
        }],
        child_threads: vec![ObservedCodexThread {
            thread: CodexThread {
                id: "child".into(),
                title: Some("Child".into()),
                cwd: Some("/tmp/project".into()),
                source: Some(
                    "{\"subagent\":{\"thread_spawn\":{\"parent_thread_id\":\"parent\"}}}".into(),
                ),
                model: Some("gpt-5".into()),
                git_branch: Some("main".into()),
                agent_nickname: Some("Boyle".into()),
                agent_role: Some("worker".into()),
                created_at: 11,
                updated_at: 21,
                archived: 0,
                tokens_used: None,
                rollout_path: Some(child_rollout.to_string_lossy().to_string()),
            },
            agent_id: AgentId::session(AgentTool::Codex, "child"),
            pid: None,
            classification: CodexThreadClassification::Child {
                parent_thread_id: Some("parent".into()),
            },
        }],
    };
    let spawn_edges = vec![SpawnEdge {
        parent_thread_id: "parent".into(),
        child_thread_id: "child".into(),
        status: "closed".into(),
    }];

    let event_snapshot = build_codex_event_snapshot(&snapshot, &spawn_edges);

    assert_eq!(event_snapshot.sessions.len(), 1);
    assert_eq!(event_snapshot.sessions[0].subagents.len(), 1);
    assert_eq!(event_snapshot.sessions[0].subagents[0].name, "Boyle");
    assert_eq!(
        event_snapshot.sessions[0].subagents[0].status,
        PrimaryStatus::Done
    );

    std::fs::remove_file(child_rollout).unwrap();
}
