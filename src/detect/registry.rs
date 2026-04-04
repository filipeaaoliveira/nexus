use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use chrono::{DateTime, Local};

use crate::types::AgentTool;

type AgentKey = (AgentTool, PathBuf);
type PidList = Vec<u32>;
type AgentMap = HashMap<AgentKey, PidList>;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ClaudeLaunchInfo {
    pub started_at: Option<DateTime<Local>>,
    pub resume_session_id: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CodexLaunchInfo {
    pub started_at: Option<DateTime<Local>>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OpenCodeLaunchInfo {
    pub started_at: Option<DateTime<Local>>,
}

/// Shared registry of known agents, mapping (tool, cwd) → one or more pids.
/// Populated by the process scanner, read by enrichment detectors.
#[derive(Debug, Clone, Default)]
pub struct AgentRegistry {
    inner: Arc<RwLock<AgentMap>>,
    codex_thread_pids: Arc<RwLock<HashMap<String, u32>>>,
    codex_launches: Arc<RwLock<HashMap<u32, CodexLaunchInfo>>>,
    claude_launches: Arc<RwLock<HashMap<u32, ClaudeLaunchInfo>>>,
    opencode_launches: Arc<RwLock<HashMap<u32, OpenCodeLaunchInfo>>>,
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

    /// Register an exact Codex thread → pid binding discovered from the
    /// process command line (for example `codex resume <thread-id>`).
    pub fn register_codex_thread(&self, thread_id: String, pid: u32) {
        let mut map = self.codex_thread_pids.write().unwrap();
        map.insert(thread_id, pid);
    }

    /// Register Codex launch metadata discovered from the process command line.
    pub fn register_codex_launch(&self, pid: u32, started_at: Option<DateTime<Local>>) {
        let mut map = self.codex_launches.write().unwrap();
        map.insert(pid, CodexLaunchInfo { started_at });
    }

    /// Register Claude launch metadata discovered from the process command line.
    pub fn register_claude_launch(
        &self,
        pid: u32,
        started_at: Option<DateTime<Local>>,
        resume_session_id: Option<String>,
    ) {
        let mut map = self.claude_launches.write().unwrap();
        map.insert(
            pid,
            ClaudeLaunchInfo {
                started_at,
                resume_session_id,
            },
        );
    }

    /// Register OpenCode launch metadata discovered from the process command line.
    pub fn register_opencode_launch(&self, pid: u32, started_at: Option<DateTime<Local>>) {
        let mut map = self.opencode_launches.write().unwrap();
        map.insert(pid, OpenCodeLaunchInfo { started_at });
    }

    /// Find launch metadata for an OpenCode process by pid.
    pub fn find_opencode_launch(&self, pid: u32) -> Option<OpenCodeLaunchInfo> {
        let map = self.opencode_launches.read().unwrap();
        map.get(&pid).cloned()
    }

    /// Remove an agent by pid. Called when process exits.
    pub fn remove(&self, pid: u32) {
        let mut map = self.inner.write().unwrap();
        map.retain(|_, pids| {
            pids.retain(|current_pid| *current_pid != pid);
            !pids.is_empty()
        });
        let mut codex_map = self.codex_thread_pids.write().unwrap();
        codex_map.retain(|_, current_pid| *current_pid != pid);
        let mut codex_launches = self.codex_launches.write().unwrap();
        codex_launches.remove(&pid);
        let mut claude_map = self.claude_launches.write().unwrap();
        claude_map.remove(&pid);
        let mut opencode_launches = self.opencode_launches.write().unwrap();
        opencode_launches.remove(&pid);
    }

    /// Find a PID by tool and CWD. Used by enrichment detectors.
    ///
    /// For Codex, this may return `None` when multiple processes share the
    /// same CWD and the binding would be ambiguous.
    ///
    /// For other tools (OpenCode, Claude), uses prefix matching: returns a PID
    /// if the process CWD starts with the session directory (handles cases where
    /// session was created in a parent directory but process runs in a subdirectory).
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

    /// Find a PID by tool and CWD with prefix matching.
    ///
    /// Returns a PID if any process with the given tool has a CWD that is the
    /// same as `cwd` OR is a subdirectory of `cwd` (starts with `cwd` + path separator).
    /// This handles cases where OpenCode sessions are created in a parent directory
    /// but the actual process runs in a subdirectory.
    pub fn find_pid_prefix(&self, tool: AgentTool, cwd: &Path) -> Option<u32> {
        let map = self.inner.read().unwrap();

        // First try exact match
        if let Some(pids) = map.get(&(tool, cwd.to_path_buf())) {
            return pids.first().copied();
        }

        // Then try prefix matching: find any CWD that starts with cwd + "/"
        let cwd_str = cwd.to_string_lossy();
        let cwd_prefix = format!("{}/", cwd_str);
        for ((registered_tool, registered_cwd), pids) in map.iter() {
            if *registered_tool != tool {
                continue;
            }
            let registered_str = registered_cwd.to_string_lossy();
            if registered_str.starts_with(&*cwd_str)
                && registered_str.len() > cwd_str.len()
                && registered_str.starts_with(&cwd_prefix)
            {
                return pids.first().copied();
            }
        }

        None
    }

    /// Find all PIDs by tool and CWD.
    pub fn find_pids(&self, tool: AgentTool, cwd: &Path) -> Vec<u32> {
        let map = self.inner.read().unwrap();
        map.get(&(tool, cwd.to_path_buf()))
            .cloned()
            .unwrap_or_default()
    }

    /// Find an exact pid for a known Codex thread id.
    pub fn find_codex_pid_for_thread(&self, thread_id: &str) -> Option<u32> {
        let map = self.codex_thread_pids.read().unwrap();
        map.get(thread_id).copied()
    }

    /// Find launch metadata for a Codex process by pid.
    pub fn find_codex_launch(&self, pid: u32) -> Option<CodexLaunchInfo> {
        let map = self.codex_launches.read().unwrap();
        map.get(&pid).cloned()
    }

    /// Find launch metadata for a Claude process by pid.
    pub fn find_claude_launch(&self, pid: u32) -> Option<ClaudeLaunchInfo> {
        let map = self.claude_launches.read().unwrap();
        map.get(&pid).cloned()
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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Local;

    #[test]
    fn test_opencode_launch_register_and_find() {
        let registry = AgentRegistry::new();
        let now = Local::now();
        registry.register_opencode_launch(99, Some(now));
        let info = registry
            .find_opencode_launch(99)
            .expect("should find launch info");
        assert_eq!(info.started_at, Some(now));
    }

    #[test]
    fn test_opencode_launch_removed_on_remove() {
        let registry = AgentRegistry::new();
        let now = Local::now();
        registry.register_opencode_launch(99, Some(now));
        registry.remove(99);
        assert!(registry.find_opencode_launch(99).is_none());
    }

    #[test]
    fn test_opencode_launch_none_started_at() {
        let registry = AgentRegistry::new();
        registry.register_opencode_launch(100, None);
        let info = registry.find_opencode_launch(100).expect("should find");
        assert!(info.started_at.is_none());
    }
}
