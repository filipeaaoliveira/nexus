use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use crate::types::AgentTool;

/// Shared registry of known agents, mapping (tool, cwd) → pid.
/// Populated by the process scanner, read by enrichment detectors.
#[derive(Debug, Clone, Default)]
pub struct AgentRegistry {
    inner: Arc<RwLock<HashMap<(AgentTool, PathBuf), u32>>>,
}

impl AgentRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a discovered agent. Called by process scanner.
    pub fn register(&self, tool: AgentTool, cwd: PathBuf, pid: u32) {
        let mut map = self.inner.write().unwrap();
        map.insert((tool, cwd), pid);
    }

    /// Remove an agent by pid. Called when process exits.
    pub fn remove(&self, pid: u32) {
        let mut map = self.inner.write().unwrap();
        map.retain(|_, v| *v != pid);
    }

    /// Find a PID by tool and CWD. Used by enrichment detectors.
    pub fn find_pid(&self, tool: AgentTool, cwd: &Path) -> Option<u32> {
        let map = self.inner.read().unwrap();
        map.get(&(tool, cwd.to_path_buf())).copied()
    }

    /// Get all registered agents for a given tool.
    pub fn agents_for_tool(&self, tool: AgentTool) -> Vec<(PathBuf, u32)> {
        let map = self.inner.read().unwrap();
        map.iter()
            .filter(|((t, _), _)| *t == tool)
            .map(|((_, cwd), pid)| (cwd.clone(), *pid))
            .collect()
    }
}
