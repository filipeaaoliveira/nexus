use ai_overview::detect::codex::*;
use ai_overview::detect::registry::AgentRegistry;
use ai_overview::event::AgentEvent;
use ai_overview::types::{AgentId, AgentTool, PrimaryStatus};
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
        started_at: ai_overview::types::from_epoch_secs(10),
        status,
        latest_tool_call: None,
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
fn test_build_codex_snapshot_does_not_bind_pid_for_stale_unique_top_level_thread_without_rollout()
{
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
        AgentEvent::ToolCallStarted { id, tool_name, .. }
            if id == &AgentId::session(AgentTool::Codex, "session-a")
                && tool_name == "shell"
    ));
    assert!(matches!(
        &events[4],
        AgentEvent::SubagentSpawned { id, subagent, .. }
            if id == &AgentId::session(AgentTool::Codex, "session-a")
                && subagent.name == "Nash"
                && subagent.status == PrimaryStatus::Running
    ));
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
        AgentEvent::ToolCallCompleted { id, tool_name, .. }
            if id == &AgentId::session(AgentTool::Codex, "session-a")
                && tool_name == "shell"
    ));
    assert!(matches!(
        &events[1],
        AgentEvent::ToolCallStarted { id, tool_name, .. }
            if id == &AgentId::session(AgentTool::Codex, "session-a")
                && tool_name == "apply_patch"
    ));
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
            }],
            ..observed_session("session-a", Some(41), PrimaryStatus::Running)
        }],
    };

    let events = reconcile_codex_snapshots(Some(&previous), &current);

    assert_eq!(events.len(), 1);
    assert!(matches!(
        &events[0],
        AgentEvent::SubagentCompleted { id, subagent_name, .. }
            if id == &AgentId::session(AgentTool::Codex, "session-a")
                && subagent_name == "child-a"
    ));
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
                },
                ObservedCodexSubagent {
                    thread_id: "child-b".to_string(),
                    name: "Nash".to_string(),
                    description: Some("child-b".to_string()),
                    status: PrimaryStatus::Running,
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
                },
                ObservedCodexSubagent {
                    thread_id: "child-b".to_string(),
                    name: "Nash".to_string(),
                    description: Some("child-b".to_string()),
                    status: PrimaryStatus::Done,
                },
            ],
            ..observed_session("session-a", Some(41), PrimaryStatus::Running)
        }],
    };

    let events = reconcile_codex_snapshots(Some(&previous), &current);

    assert_eq!(events.len(), 1);
    assert!(matches!(
        &events[0],
        AgentEvent::SubagentCompleted { subagent_name, .. }
            if subagent_name == "child-b"
    ));
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
