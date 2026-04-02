use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use crate::types::AgentTool;

/// Shared registry of known agents, mapping (tool, cwd) → one or more pids.
/// Populated by the process scanner, read by enrichment detectors.
#[derive(Debug, Clone, Default)]
pub struct AgentRegistry {
    inner: Arc<RwLock<HashMap<(AgentTool, PathBuf), Vec<u32>>>>,
}

impl AgentRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a discovered agent. Called by process scanner.
    pub fn register(&self, tool: AgentTool, cwd: PathBuf, pid: u32) {
        let mut map = self.inner.write().unwrap();
        let entry = map.entry((tool, cwd)).or_default();
        if tool == AgentTool::Codex {
            if !entry.contains(&pid) {
                entry.push(pid);
            }
        } else {
            entry.clear();
            entry.push(pid);
        }
    }

    /// Remove an agent by pid. Called when process exits.
    pub fn remove(&self, pid: u32) {
        let mut map = self.inner.write().unwrap();
        map.retain(|_, pids| {
            pids.retain(|current_pid| *current_pid != pid);
            !pids.is_empty()
        });
    }

    /// Find a PID by tool and CWD. Used by enrichment detectors.
    ///
    /// For Codex, this may return `None` when multiple processes share the
    /// same CWD and the binding would be ambiguous.
    pub fn find_pid(&self, tool: AgentTool, cwd: &Path) -> Option<u32> {
        let map = self.inner.read().unwrap();
        let pids = map.get(&(tool, cwd.to_path_buf()))?;
        if tool == AgentTool::Codex {
            match pids.as_slice() {
                [pid] => Some(*pid),
                _ => None,
            }
        } else {
            pids.first().copied()
        }
    }

    /// Find all PIDs by tool and CWD.
    pub fn find_pids(&self, tool: AgentTool, cwd: &Path) -> Vec<u32> {
        let map = self.inner.read().unwrap();
        map.get(&(tool, cwd.to_path_buf()))
            .cloned()
            .unwrap_or_default()
    }

    /// Get all registered agents for a given tool.
    pub fn agents_for_tool(&self, tool: AgentTool) -> Vec<(PathBuf, u32)> {
        let map = self.inner.read().unwrap();
        let mut agents = Vec::new();
        for ((registered_tool, cwd), pids) in map.iter() {
            if *registered_tool != tool {
                continue;
            }
            for pid in pids {
                agents.push((cwd.clone(), *pid));
            }
        }
        agents
    }
}
