pub mod claude;
pub mod codex;
pub mod opencode;
pub mod process;
pub mod registry;
pub mod worktree;

use crate::config::Config;
use crate::event::AgentEvent;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::error;

/// Run a detector in a loop with exponential backoff on failure.
async fn run_with_restart<F, Fut>(name: &str, mut make_future: F)
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = anyhow::Result<()>>,
{
    let mut backoff = Duration::from_secs(1);
    let max_backoff = Duration::from_secs(60);
    loop {
        match make_future().await {
            Ok(()) => break,
            Err(e) => {
                error!("{name} failed: {e:#}, restarting in {backoff:?}");
                tokio::time::sleep(backoff).await;
                backoff = (backoff * 2).min(max_backoff);
            }
        }
    }
}

/// Spawn all detector tasks. Returns the receiver end of the event channel.
pub fn spawn_detectors(config: &Config) -> mpsc::Receiver<AgentEvent> {
    let (tx, rx) = mpsc::channel::<AgentEvent>(512);
    let poll = Duration::from_secs(config.poll_interval_secs);
    let registry = registry::AgentRegistry::new();

    // Process scanner — pass registry so it registers discovered agents
    let process_tx = tx.clone();
    let process_registry = registry.clone();
    tokio::spawn(async move {
        run_with_restart("Process scanner", || {
            process::run_process_scanner(process_tx.clone(), poll, process_registry.clone())
        })
        .await;
    });

    // Claude detector — pass registry so it can resolve CWD→PID
    let claude_tx = tx.clone();
    let claude_dir = config.claude_data_dir.clone();
    let claude_registry = registry.clone();
    tokio::spawn(async move {
        run_with_restart("Claude detector", || {
            claude::run_claude_detector(
                claude_tx.clone(),
                claude_dir.clone(),
                poll,
                claude_registry.clone(),
            )
        })
        .await;
    });

    // OpenCode detector
    let oc_tx = tx.clone();
    let oc_dir = config.opencode_data_dir.clone();
    let oc_registry = registry.clone();
    tokio::spawn(async move {
        run_with_restart("OpenCode detector", || {
            opencode::run_opencode_detector(
                oc_tx.clone(),
                oc_dir.clone(),
                poll,
                oc_registry.clone(),
            )
        })
        .await;
    });

    // Codex detector
    let codex_tx = tx.clone();
    let codex_dir = config.codex_data_dir.clone();
    let codex_registry = registry.clone();
    tokio::spawn(async move {
        run_with_restart("Codex detector", || {
            codex::run_codex_detector(
                codex_tx.clone(),
                codex_dir.clone(),
                poll,
                codex_registry.clone(),
            )
        })
        .await;
    });

    rx
}
