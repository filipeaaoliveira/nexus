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

/// Spawn all detector tasks. Returns the receiver end of the event channel.
pub fn spawn_detectors(config: &Config) -> mpsc::Receiver<AgentEvent> {
    let (tx, rx) = mpsc::channel::<AgentEvent>(512);
    let poll = Duration::from_secs(config.poll_interval_secs);
    let registry = registry::AgentRegistry::new();

    // Process scanner — pass registry so it registers discovered agents
    let process_tx = tx.clone();
    let process_registry = registry.clone();
    tokio::spawn(async move {
        if let Err(e) = process::run_process_scanner(process_tx, poll, process_registry).await {
            error!("Process scanner failed: {e}");
        }
    });

    // Claude detector — pass registry so it can resolve CWD→PID
    let claude_tx = tx.clone();
    let claude_dir = config.claude_data_dir.clone();
    let claude_registry = registry.clone();
    tokio::spawn(async move {
        if let Err(e) =
            claude::run_claude_detector(claude_tx, claude_dir, poll, claude_registry).await
        {
            error!("Claude detector failed: {e}");
        }
    });

    // OpenCode detector
    let oc_tx = tx.clone();
    let oc_dir = config.opencode_data_dir.clone();
    let oc_registry = registry.clone();
    tokio::spawn(async move {
        if let Err(e) =
            opencode::run_opencode_detector(oc_tx, oc_dir, poll, oc_registry).await
        {
            error!("OpenCode detector failed: {e}");
        }
    });

    // Codex detector
    let codex_tx = tx.clone();
    let codex_dir = config.codex_data_dir.clone();
    let codex_registry = registry.clone();
    tokio::spawn(async move {
        if let Err(e) =
            codex::run_codex_detector(codex_tx, codex_dir, poll, codex_registry).await
        {
            error!("Codex detector failed: {e}");
        }
    });

    rx
}
