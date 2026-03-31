pub mod process;
pub mod worktree;

use crate::event::AgentEvent;
use tokio::sync::mpsc;

/// Trait for agent detectors. Each runs as an independent async task.
pub trait AgentDetector: Send + Sync {
    fn run(
        &self,
        tx: mpsc::Sender<AgentEvent>,
    ) -> impl std::future::Future<Output = anyhow::Result<()>> + Send;
}
