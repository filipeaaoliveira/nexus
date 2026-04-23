pub mod claude;
pub mod codex;
pub mod error;
pub(crate) mod heuristics;
pub mod opencode;
pub mod process;
pub mod registry;
pub mod worktree;

use crate::config::Config;
use crate::event::{AgentEvent, AgentRef};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::sync::Mutex;
use tracing::error;

use nexus_domain::agents::ids::AgentId;
use nexus_domain::system::DetectorKind;

#[derive(Debug, Clone)]
struct CachedWorktreeInfo {
    branch: Option<String>,
    is_worktree: bool,
    cached_at: std::time::Instant,
}

type WorktreeCache = Arc<Mutex<HashMap<PathBuf, CachedWorktreeInfo>>>;

async fn lookup_branch_info_cached(
    cwd: PathBuf,
    cache_ttl: Duration,
    cache: WorktreeCache,
) -> (Option<String>, bool) {
    {
        let cache_guard = cache.lock().await;
        if let Some(entry) = cache_guard.get(&cwd) {
            if entry.cached_at.elapsed() <= cache_ttl {
                return (entry.branch.clone(), entry.is_worktree);
            }
        }
    }

    let (branch, is_worktree) = worktree::lookup_branch_info(&cwd).await;
    let mut cache_guard = cache.lock().await;
    cache_guard.insert(
        cwd,
        CachedWorktreeInfo {
            branch: branch.clone(),
            is_worktree,
            cached_at: std::time::Instant::now(),
        },
    );
    (branch, is_worktree)
}

async fn emit_enriched_session_info(
    tx: mpsc::Sender<AgentEvent>,
    cache: WorktreeCache,
    cache_ttl: Duration,
    id: AgentId,
    pid: Option<u32>,
    cwd: PathBuf,
    started_at: Option<chrono::DateTime<chrono::Utc>>,
) {
    let (branch, is_worktree) = lookup_branch_info_cached(cwd, cache_ttl, cache).await;
    let _ = tx
        .send(AgentEvent::SessionInfo {
            agent: AgentRef::new(id, pid),
            session_id: None,
            branch,
            is_worktree,
            started_at,
        })
        .await;
}

async fn forward_agent_events(
    mut rx: mpsc::Receiver<AgentEvent>,
    tx: mpsc::Sender<AgentEvent>,
    cache_ttl: Duration,
) {
    let worktree_cache: WorktreeCache = Arc::new(Mutex::new(HashMap::new()));
    let mut enrichment_tasks = tokio::task::JoinSet::new();

    loop {
        tokio::select! {
            event = rx.recv() => {
                let Some(event) = event else { break };

                if let AgentEvent::Discovered {
                    agent,
                    cwd,
                    started_at,
                } = &event
                {
                    let enrichment_tx = tx.clone();
                    let enrichment_cache = worktree_cache.clone();
                    let id = agent.id.clone();
                    let pid = agent.pid;
                    let cwd = cwd.clone();
                    let started_at = *started_at;
                    enrichment_tasks.spawn(async move {
                        emit_enriched_session_info(
                            enrichment_tx,
                            enrichment_cache,
                            cache_ttl,
                            id,
                            pid,
                            cwd,
                            started_at,
                        )
                        .await;
                    });
                }

                if tx.send(event).await.is_err() {
                    break;
                }
            }
            // Drain completed enrichment tasks to avoid unbounded growth
            Some(_result) = enrichment_tasks.join_next(), if !enrichment_tasks.is_empty() => {}
        }
    }

    // Drain in-flight enrichment tasks (don't abort — let them deliver results)
    while enrichment_tasks.join_next().await.is_some() {}
}

/// Run a detector in a loop with exponential backoff on failure.
///
/// Detectors are written to run indefinitely and only return `Ok(())` when
/// their send channel is dropped (i.e. when the tokio runtime shuts down).
/// The `Ok(())` arm therefore acts as a clean-exit sentinel rather than an
/// error path: `break` there lets the spawned task finish without logging a
/// spurious restart. The restart logic is exercised only on `Err`, which
/// covers real detector failures (e.g. SQLite open errors on startup).
async fn run_with_restart<F, Fut>(name: &str, mut make_future: F)
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<(), error::DetectorError>>,
{
    let mut backoff = Duration::from_secs(1);
    let max_backoff = Duration::from_secs(60);
    loop {
        match make_future().await {
            Ok(()) => break,
            Err(e) => {
                error!("{name} failed: {e}, restarting in {backoff:?}");
                tokio::time::sleep(backoff).await;
                backoff = (backoff * 2).min(max_backoff);
            }
        }
    }
}

/// Like [`run_with_restart`], but also emits a [`AgentEvent::DetectorHealthChanged`]
/// event on the first failure (healthy → unhealthy transition).
async fn run_with_restart_reporting<F, Fut>(
    name: &str,
    detector: DetectorKind,
    health_tx: mpsc::Sender<AgentEvent>,
    mut make_future: F,
) where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<(), error::DetectorError>>,
{
    let mut backoff = Duration::from_secs(1);
    let max_backoff = Duration::from_secs(60);
    let mut reported_unhealthy = false;
    loop {
        match make_future().await {
            Ok(()) => break,
            Err(e) => {
                let transient = e.is_transient();
                error!("{name} failed: {e}, transient={transient}");
                if !reported_unhealthy {
                    reported_unhealthy = true;
                    let _ = health_tx
                        .send(AgentEvent::DetectorHealthChanged {
                            detector,
                            healthy: false,
                            message: format!("{name} failed: {e}"),
                        })
                        .await;
                }
                if !transient {
                    error!("{name} hit permanent error, stopping");
                    break;
                }
                tokio::time::sleep(backoff).await;
                backoff = (backoff * 2).min(max_backoff);
            }
        }
    }
}

/// Owned handles for all detector background tasks.
///
/// Aborting on drop ensures detectors do not outlive the event channel.
pub struct DetectorTasks {
    forward: tokio::task::JoinHandle<()>,
    process: tokio::task::JoinHandle<()>,
    claude: tokio::task::JoinHandle<()>,
    opencode: tokio::task::JoinHandle<()>,
    codex: tokio::task::JoinHandle<()>,
}

impl Drop for DetectorTasks {
    fn drop(&mut self) {
        self.forward.abort();
        self.process.abort();
        self.claude.abort();
        self.opencode.abort();
        self.codex.abort();
    }
}

/// Spawn all detector tasks. Returns owned task handles and the event receiver.
pub fn spawn_detectors(config: &Config) -> (DetectorTasks, mpsc::Receiver<AgentEvent>) {
    let (raw_tx, raw_rx) = mpsc::channel::<AgentEvent>(512);
    let (tx, rx) = mpsc::channel::<AgentEvent>(512);
    let poll = Duration::from_secs(config.poll_interval_secs);
    let worktree_cache_ttl = Duration::from_secs(config.worktree_scan_interval_secs);
    let registry = registry::RegistryHandle::spawn();

    let forward = tokio::spawn(forward_agent_events(raw_rx, tx.clone(), worktree_cache_ttl));

    // Process scanner — pass registry so it registers discovered agents
    let process_tx = raw_tx.clone();
    let process_registry = registry.clone();
    let process = tokio::spawn(async move {
        run_with_restart("Process scanner", || {
            process::run_process_scanner(process_tx.clone(), poll, process_registry.clone())
        })
        .await;
    });

    // Claude detector — pass registry so it can resolve CWD→PID
    let claude_tx = raw_tx.clone();
    let claude_health_tx = raw_tx.clone();
    let claude_dir = config.claude_data_dir.clone();
    let claude_registry = registry.clone();
    let claude = tokio::spawn(async move {
        run_with_restart_reporting(
            "Claude detector",
            DetectorKind::Claude,
            claude_health_tx,
            || {
                claude::run_claude_detector(
                    claude_tx.clone(),
                    claude_dir.clone(),
                    poll,
                    claude_registry.clone(),
                )
            },
        )
        .await;
    });

    // OpenCode detector
    let oc_tx = raw_tx.clone();
    let oc_health_tx = raw_tx.clone();
    let oc_dir = config.opencode_data_dir.clone();
    let oc_registry = registry.clone();
    let opencode = tokio::spawn(async move {
        run_with_restart_reporting(
            "OpenCode detector",
            DetectorKind::OpenCode,
            oc_health_tx,
            || {
                opencode::run_opencode_detector(
                    oc_tx.clone(),
                    oc_dir.clone(),
                    poll,
                    oc_registry.clone(),
                )
            },
        )
        .await;
    });

    // Codex detector
    let codex_tx = raw_tx.clone();
    let codex_health_tx = raw_tx.clone();
    let codex_dir = config.codex_data_dir.clone();
    let codex_registry = registry.clone();
    let codex = tokio::spawn(async move {
        run_with_restart_reporting(
            "Codex detector",
            DetectorKind::Codex,
            codex_health_tx,
            || {
                codex::run_codex_detector(
                    codex_tx.clone(),
                    codex_dir.clone(),
                    poll,
                    codex_registry.clone(),
                )
            },
        )
        .await;
    });

    let tasks = DetectorTasks {
        forward,
        process,
        claude,
        opencode,
        codex,
    };
    (tasks, rx)
}

#[cfg(test)]
mod tests {
    use super::forward_agent_events;
    use crate::event::{AgentEvent, AgentRef};
    use chrono::Utc;
    use nexus_domain::agents::ids::{AgentId, AgentTool};
    use std::path::PathBuf;
    use std::time::Duration;
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn test_forward_agent_events_preserves_discovered_and_emits_session_info() {
        let (input_tx, input_rx) = mpsc::channel(8);
        let (output_tx, mut output_rx) = mpsc::channel(8);

        tokio::spawn(forward_agent_events(
            input_rx,
            output_tx,
            Duration::from_secs(60),
        ));

        let event = AgentEvent::Discovered {
            agent: AgentRef::new(AgentId::session(AgentTool::Codex, "thread-1"), Some(42)),
            cwd: PathBuf::from("/definitely/not/a/repo"),
            started_at: Some(Utc::now()),
        };
        input_tx.send(event.clone()).await.expect("send discovered");
        drop(input_tx);

        let first = output_rx.recv().await.expect("first event");
        let second = output_rx.recv().await.expect("second event");
        let events = [first, second];

        assert!(
            events
                .iter()
                .any(|item| matches!(item, AgentEvent::Discovered { .. })),
            "expected original discovered event"
        );
        assert!(
            events
                .iter()
                .any(|item| matches!(item, AgentEvent::SessionInfo { .. })),
            "expected detector-side session enrichment"
        );
    }

    #[tokio::test]
    async fn run_with_restart_reporting_emits_health_on_failure() {
        use crate::event::AgentEvent;
        use nexus_domain::system::DetectorKind;
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;

        let call_count = Arc::new(AtomicUsize::new(0));
        let (health_tx, mut health_rx) = mpsc::channel::<AgentEvent>(8);

        let count = call_count.clone();
        super::run_with_restart_reporting("test-detector", DetectorKind::Codex, health_tx, || {
            let n = count.fetch_add(1, Ordering::SeqCst);
            let result: Result<(), super::error::DetectorError> = if n == 0 {
                Err(super::error::DetectorError::io(
                    DetectorKind::Codex,
                    "test",
                    std::io::Error::new(std::io::ErrorKind::Interrupted, "boom"),
                ))
            } else {
                Ok(())
            };
            async move { result }
        })
        .await;

        let event = health_rx.try_recv().expect("expected health event");
        assert!(matches!(
            event,
            AgentEvent::DetectorHealthChanged { healthy: false, .. }
        ));
    }

    #[tokio::test]
    async fn detector_tasks_drop_aborts_all_child_tasks() {
        use super::DetectorTasks;

        let make_task = || {
            tokio::spawn(async {
                // Simulate a long-running detector
                tokio::time::sleep(Duration::from_secs(600)).await;
            })
        };

        let forward = make_task();
        let process = make_task();
        let claude = make_task();
        let opencode = make_task();
        let codex = make_task();

        // Clone abort handles before moving into DetectorTasks
        let handles: Vec<_> = [&forward, &process, &claude, &opencode, &codex]
            .iter()
            .map(|h| h.abort_handle())
            .collect();

        let tasks = DetectorTasks {
            forward,
            process,
            claude,
            opencode,
            codex,
        };

        drop(tasks);

        // Yield to let the runtime process abort signals
        tokio::task::yield_now().await;

        for (i, handle) in handles.iter().enumerate() {
            assert!(handle.is_finished(), "task {i} was not aborted by Drop");
        }
    }
}
