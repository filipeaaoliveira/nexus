use std::path::PathBuf;
use std::time::Duration;

use tokio::sync::mpsc;

use crate::detect::registry::RegistryHandle;
use crate::event::AgentEvent;

#[cfg(test)]
mod integration_tests;
pub(crate) mod normalization;
pub(crate) mod pid_binding;
pub(crate) mod reconcile;
mod rollout;
mod snapshot;
mod sqlite;
mod types;

#[cfg(test)]
pub use normalization::normalize_codex_session;
#[cfg(test)]
pub(crate) use pid_binding::codex_pid_for_thread;
#[cfg(test)]
pub use rollout::*;
pub use rollout::{parse_rollout, parse_session_index};
#[cfg(test)]
pub use snapshot::*;
#[allow(hidden_glob_reexports)]
use reconcile::reconcile_codex_snapshots;
#[allow(hidden_glob_reexports)]
use snapshot::detect_codex_backend_with_history;
pub use sqlite::parse_source_field;
#[allow(hidden_glob_reexports)]
use types::CodexEventSnapshot;
#[cfg(test)]
pub use types::*;

const CONSERVATIVE_PID_BINDING_WINDOW_SECS: i64 = 300;
const SAME_CWD_MULTI_BIND_MAX_SKEW_SECS: i64 = 90;

// ── Detector Loop ─────────────────────────────────────────────────

/// Run the Codex detector polling loop.
///
/// Watches the Codex data directory for state changes by polling the
/// SQLite database at `codex_data_dir/state_5.sqlite`.
pub async fn run_codex_detector(
    tx: mpsc::Sender<AgentEvent>,
    codex_data_dir: PathBuf,
    poll_interval: Duration,
    registry: RegistryHandle,
) -> Result<(), crate::detect::error::DetectorError> {
    let db_path = codex_data_dir.join("state_5.sqlite");
    let mut previous_snapshot: Option<CodexEventSnapshot> = None;
    let mut has_seen_db = false;

    loop {
        let db = db_path.clone();
        let registry_snap = registry.snapshot().await;
        if db_path.exists() {
            has_seen_db = true;
        }
        let result = tokio::task::spawn_blocking(move || {
            detect_codex_backend_with_history(&db, &registry_snap, has_seen_db)
        })
        .await
        .map_err(|e| crate::detect::error::DetectorError::io(nexus_domain::system::DetectorKind::Codex, "spawn_blocking join", std::io::Error::other(e.to_string())))?;

        let current_snapshot = result.snapshot;
        let events = reconcile_codex_snapshots(previous_snapshot.as_ref(), &current_snapshot);

        for event in events {
            let _ = tx.send(event).await;
        }

        previous_snapshot = Some(current_snapshot);

        tokio::time::sleep(poll_interval).await;
    }
}

#[cfg(test)]
mod tests {
    use super::{
        build_codex_event_snapshot, codex_pid_for_thread, load_spawn_edges_snapshot,
        snapshot_child_threads_by_parent, CodexEventSnapshot, CodexSnapshot, CodexThread,
        CodexThreadClassification, ObservedCodexSession, ObservedCodexSubagent,
        ObservedCodexThread,
    };
    use crate::detect::codex::reconcile::reconcile_codex_snapshots;
    use crate::detect::registry::RegistrySnapshot;
    use crate::event::AgentEvent;
    use nexus_domain::agents::ids::{AgentId, AgentTool};
    use nexus_domain::agents::status::PrimaryStatus;
    use nexus_domain::shared::time::from_epoch_secs;
    use rusqlite::Connection;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn codex_pid_for_thread_returns_single_binding_only() {
        let mut registry = RegistrySnapshot::new();
        let cwd = PathBuf::from("/tmp/project");

        assert_eq!(codex_pid_for_thread(&registry, &cwd), None);

        registry.register(AgentTool::Codex, cwd.clone(), 101);
        assert_eq!(codex_pid_for_thread(&registry, &cwd), Some(101));

        registry.register(AgentTool::Codex, cwd.clone(), 202);
        assert_eq!(codex_pid_for_thread(&registry, &cwd), None);
    }

    #[test]
    fn snapshot_child_threads_by_parent_uses_classification() {
        let snapshot = CodexSnapshot {
            top_level_threads: vec![ObservedCodexThread {
                thread: CodexThread {
                    id: "parent".into(),
                    title: None,
                    cwd: Some("/tmp/project".into()),
                    source: Some("cli".into()),
                    model: None,
                    git_branch: None,
                    agent_nickname: None,
                    agent_role: None,
                    created_at: 1,
                    updated_at: 1,
                    archived: 0,
                    tokens_used: None,
                    rollout_path: None,
                },
                agent_id: AgentId::session(AgentTool::Codex, "parent"),
                pid: None,
                classification: CodexThreadClassification::TopLevel,
            }],
            child_threads: vec![
                ObservedCodexThread {
                    thread: CodexThread {
                        id: "child-a".into(),
                        title: None,
                        cwd: Some("/tmp/project".into()),
                        source: Some("cli".into()),
                        model: None,
                        git_branch: None,
                        agent_nickname: None,
                        agent_role: None,
                        created_at: 2,
                        updated_at: 2,
                        archived: 0,
                        tokens_used: None,
                        rollout_path: None,
                    },
                    agent_id: AgentId::session(AgentTool::Codex, "child-a"),
                    pid: None,
                    classification: CodexThreadClassification::Child {
                        parent_thread_id: Some("parent".into()),
                    },
                },
                ObservedCodexThread {
                    thread: CodexThread {
                        id: "orphan".into(),
                        title: None,
                        cwd: Some("/tmp/project".into()),
                        source: Some("cli".into()),
                        model: None,
                        git_branch: None,
                        agent_nickname: None,
                        agent_role: None,
                        created_at: 3,
                        updated_at: 3,
                        archived: 0,
                        tokens_used: None,
                        rollout_path: None,
                    },
                    agent_id: AgentId::session(AgentTool::Codex, "orphan"),
                    pid: None,
                    classification: CodexThreadClassification::Child {
                        parent_thread_id: None,
                    },
                },
            ],
        };

        let grouped = snapshot_child_threads_by_parent(&snapshot);

        assert_eq!(grouped.len(), 1);
        assert_eq!(grouped["parent"][0].thread.id, "child-a");
    }

    #[test]
    fn load_spawn_edges_snapshot_ignores_missing_table() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let db_path = std::env::temp_dir().join(format!("codex-missing-edges-{unique}.sqlite"));
        Connection::open(&db_path).unwrap();

        let edges = load_spawn_edges_snapshot(&db_path);

        assert!(edges.is_empty());

        std::fs::remove_file(db_path).unwrap();
    }

    #[test]
    fn build_codex_event_snapshot_includes_child_threads_as_subagents() {
        let snapshot = CodexSnapshot {
            top_level_threads: vec![ObservedCodexThread {
                thread: CodexThread {
                    id: "parent".into(),
                    title: None,
                    cwd: Some("/tmp/project".into()),
                    source: Some("cli".into()),
                    model: None,
                    git_branch: Some("main".into()),
                    agent_nickname: None,
                    agent_role: None,
                    created_at: 1,
                    updated_at: 1,
                    archived: 0,
                    tokens_used: None,
                    rollout_path: None,
                },
                agent_id: AgentId::session(AgentTool::Codex, "parent"),
                pid: Some(44),
                classification: CodexThreadClassification::TopLevel,
            }],
            child_threads: vec![ObservedCodexThread {
                thread: CodexThread {
                    id: "child-a".into(),
                    title: None,
                    cwd: Some("/tmp/project".into()),
                    source: Some("cli".into()),
                    model: None,
                    git_branch: None,
                    agent_nickname: Some("worker".into()),
                    agent_role: Some("worker".into()),
                    created_at: 2,
                    updated_at: 2,
                    archived: 0,
                    tokens_used: None,
                    rollout_path: None,
                },
                agent_id: AgentId::session(AgentTool::Codex, "child-a"),
                pid: None,
                classification: CodexThreadClassification::Child {
                    parent_thread_id: Some("parent".into()),
                },
            }],
        };

        let event_snapshot = build_codex_event_snapshot(&snapshot, &[]);

        assert_eq!(event_snapshot.sessions.len(), 1);
        assert_eq!(event_snapshot.sessions[0].subagents.len(), 1);
        assert_eq!(event_snapshot.sessions[0].subagents[0].thread_id, "child-a");
        assert_eq!(event_snapshot.sessions[0].subagents[0].name, "worker");
    }

    #[test]
    fn reconcile_codex_snapshots_emits_subagent_completion_delta() {
        let previous = CodexEventSnapshot {
            sessions: vec![ObservedCodexSession {
                agent_id: AgentId::session(AgentTool::Codex, "parent"),
                pid: Some(44),
                cwd: PathBuf::from("/tmp/project"),
                branch: Some("main".into()),
                started_at: from_epoch_secs(1),
                status: PrimaryStatus::Running,
                final_output: None,
                final_output_is_completion: false,
                latest_tool_call: None,
                todos: Vec::new(),
                subagents: vec![ObservedCodexSubagent {
                    thread_id: "child-a".into(),
                    name: "child-a".into(),
                    description: Some("worker".into()),
                    status: PrimaryStatus::Running,
                    reason: None,
                    started_at: None,
                }],
            }],
        };
        let current = CodexEventSnapshot {
            sessions: vec![ObservedCodexSession {
                subagents: vec![ObservedCodexSubagent {
                    status: PrimaryStatus::Done,
                    ..previous.sessions[0].subagents[0].clone()
                }],
                ..previous.sessions[0].clone()
            }],
        };

        let events = reconcile_codex_snapshots(Some(&previous), &current);

        assert!(events.iter().any(|event| {
            matches!(
                event,
                AgentEvent::SubagentCompleted { subagent_name, .. }
                    if subagent_name == "child-a"
            )
        }));
    }
}
