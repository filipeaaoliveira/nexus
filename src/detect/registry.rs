use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Local};

use crate::domain::agents::ids::AgentTool;

type AgentKey = (AgentTool, PathBuf);
type PidList = Vec<u32>;

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

/// All mutable state behind a single lock.
#[derive(Debug, Default)]
struct RegistryState {
    agents: HashMap<AgentKey, PidList>,
    codex_thread_pids: HashMap<String, u32>,
    codex_launches: HashMap<u32, CodexLaunchInfo>,
    claude_launches: HashMap<u32, ClaudeLaunchInfo>,
    opencode_launches: HashMap<u32, OpenCodeLaunchInfo>,
}

/// Shared registry of known agents, mapping (tool, cwd) → one or more pids.
/// Populated by the process scanner, read by enrichment detectors.
///
/// All state is behind a single `Mutex` to avoid multi-lock ordering issues
/// and simplify the concurrent access pattern.
#[derive(Debug, Clone, Default)]
pub struct AgentRegistry {
    state: Arc<Mutex<RegistryState>>,
}

impl AgentRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a discovered agent. Called by process scanner.
    pub fn register(&self, tool: AgentTool, cwd: PathBuf, pid: u32) {
        let mut state = self.state.lock().expect("registry lock");
        let entry = state.agents.entry((tool, cwd)).or_default();
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
        let mut state = self.state.lock().expect("registry lock");
        state.codex_thread_pids.insert(thread_id, pid);
    }

    /// Register Codex launch metadata discovered from the process command line.
    pub fn register_codex_launch(&self, pid: u32, started_at: Option<DateTime<Local>>) {
        let mut state = self.state.lock().expect("registry lock");
        state
            .codex_launches
            .insert(pid, CodexLaunchInfo { started_at });
    }

    /// Register Claude launch metadata discovered from the process command line.
    pub fn register_claude_launch(
        &self,
        pid: u32,
        started_at: Option<DateTime<Local>>,
        resume_session_id: Option<String>,
    ) {
        let mut state = self.state.lock().expect("registry lock");
        state.claude_launches.insert(
            pid,
            ClaudeLaunchInfo {
                started_at,
                resume_session_id,
            },
        );
    }

    /// Register OpenCode launch metadata discovered from the process command line.
    pub fn register_opencode_launch(&self, pid: u32, started_at: Option<DateTime<Local>>) {
        let mut state = self.state.lock().expect("registry lock");
        state
            .opencode_launches
            .insert(pid, OpenCodeLaunchInfo { started_at });
    }

    /// Find launch metadata for an OpenCode process by pid.
    pub fn find_opencode_launch(&self, pid: u32) -> Option<OpenCodeLaunchInfo> {
        let state = self.state.lock().expect("registry lock");
        state.opencode_launches.get(&pid).cloned()
    }

    /// Remove an agent by pid. Called when process exits.
    pub fn remove(&self, pid: u32) {
        let mut state = self.state.lock().expect("registry lock");
        state.agents.retain(|_, pids| {
            pids.retain(|current_pid| *current_pid != pid);
            !pids.is_empty()
        });
        state
            .codex_thread_pids
            .retain(|_, current_pid| *current_pid != pid);
        state.codex_launches.remove(&pid);
        state.claude_launches.remove(&pid);
        state.opencode_launches.remove(&pid);
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
        let state = self.state.lock().expect("registry lock");
        let pids = state.agents.get(&(tool, cwd.to_path_buf()))?;
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
        let state = self.state.lock().expect("registry lock");

        // First try exact match
        if let Some(pids) = state.agents.get(&(tool, cwd.to_path_buf())) {
            return pids.first().copied();
        }

        // Then try prefix matching: find any CWD that starts with cwd + "/"
        let cwd_str = cwd.to_string_lossy();
        let cwd_prefix = format!("{}/", cwd_str);
        for ((registered_tool, registered_cwd), pids) in state.agents.iter() {
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
        let state = self.state.lock().expect("registry lock");
        state
            .agents
            .get(&(tool, cwd.to_path_buf()))
            .cloned()
            .unwrap_or_default()
    }

    /// Find an exact pid for a known Codex thread id.
    pub fn find_codex_pid_for_thread(&self, thread_id: &str) -> Option<u32> {
        let state = self.state.lock().expect("registry lock");
        state.codex_thread_pids.get(thread_id).copied()
    }

    /// Find launch metadata for a Codex process by pid.
    pub fn find_codex_launch(&self, pid: u32) -> Option<CodexLaunchInfo> {
        let state = self.state.lock().expect("registry lock");
        state.codex_launches.get(&pid).cloned()
    }

    /// Find launch metadata for a Claude process by pid.
    pub fn find_claude_launch(&self, pid: u32) -> Option<ClaudeLaunchInfo> {
        let state = self.state.lock().expect("registry lock");
        state.claude_launches.get(&pid).cloned()
    }

    /// Get all registered agents for a given tool.
    pub fn agents_for_tool(&self, tool: AgentTool) -> Vec<(PathBuf, u32)> {
        let state = self.state.lock().expect("registry lock");
        let mut agents = Vec::new();
        for ((registered_tool, cwd), pids) in state.agents.iter() {
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
    use std::path::PathBuf;

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

    #[test]
    fn test_codex_registry_returns_none_for_ambiguous_cwd() {
        let registry = AgentRegistry::new();
        let cwd = PathBuf::from("/tmp/project");

        registry.register(AgentTool::Codex, cwd.clone(), 101);
        registry.register(AgentTool::Codex, cwd.clone(), 202);

        assert_eq!(registry.find_pid(AgentTool::Codex, &cwd), None);
        assert_eq!(registry.find_pids(AgentTool::Codex, &cwd), vec![101, 202]);
        assert_eq!(
            registry.agents_for_tool(AgentTool::Codex),
            vec![(cwd, 101), (PathBuf::from("/tmp/project"), 202)]
        );
    }

    #[test]
    fn test_codex_registry_returns_pid_for_single_cwd_candidate() {
        let registry = AgentRegistry::new();
        let cwd = PathBuf::from("/tmp/project");

        registry.register(AgentTool::Codex, cwd.clone(), 101);

        assert_eq!(registry.find_pid(AgentTool::Codex, &cwd), Some(101));
        assert_eq!(registry.find_pids(AgentTool::Codex, &cwd), vec![101]);
        assert_eq!(registry.agents_for_tool(AgentTool::Codex), vec![(cwd, 101)]);
    }

    #[test]
    fn test_non_codex_registry_still_overwrites_pid_for_same_cwd() {
        let registry = AgentRegistry::new();
        let cwd = PathBuf::from("/tmp/project");

        registry.register(AgentTool::Claude, cwd.clone(), 303);
        registry.register(AgentTool::Claude, cwd.clone(), 404);

        assert_eq!(registry.find_pid(AgentTool::Claude, &cwd), Some(404));
        assert_eq!(registry.find_pids(AgentTool::Claude, &cwd), vec![404]);
        assert_eq!(
            registry.agents_for_tool(AgentTool::Claude),
            vec![(cwd, 404)]
        );
    }

    #[test]
    fn test_codex_remove_only_drops_matching_pid() {
        let registry = AgentRegistry::new();
        let cwd = PathBuf::from("/tmp/project");

        registry.register(AgentTool::Codex, cwd.clone(), 101);
        registry.register(AgentTool::Codex, cwd.clone(), 202);
        registry.remove(101);

        assert_eq!(registry.find_pid(AgentTool::Codex, &cwd), Some(202));
        assert_eq!(registry.find_pids(AgentTool::Codex, &cwd), vec![202]);
        assert_eq!(registry.agents_for_tool(AgentTool::Codex), vec![(cwd, 202)]);
    }

    #[test]
    fn test_codex_registry_tracks_exact_thread_pid_binding() {
        let registry = AgentRegistry::new();
        let cwd = PathBuf::from("/tmp/project");

        registry.register(AgentTool::Codex, cwd, 15642);
        registry.register_codex_thread("019d4ffb-ca93-7c83-9cb5-72695ed739e0".to_string(), 15642);

        assert_eq!(
            registry.find_codex_pid_for_thread("019d4ffb-ca93-7c83-9cb5-72695ed739e0"),
            Some(15642)
        );

        registry.remove(15642);

        assert_eq!(
            registry.find_codex_pid_for_thread("019d4ffb-ca93-7c83-9cb5-72695ed739e0"),
            None
        );
    }

    #[test]
    fn register_codex_launch() {
        let registry = AgentRegistry::new();
        let started_at = Local::now();

        registry.register_codex_launch(4242, Some(started_at));

        let info = registry.find_codex_launch(4242).expect("launch info");
        assert_eq!(info.started_at, Some(started_at));

        registry.remove(4242);
        assert!(registry.find_codex_launch(4242).is_none());
    }

    #[test]
    fn test_registry_tracks_claude_launch_metadata() {
        let registry = AgentRegistry::new();
        let started_at = Local::now();

        registry.register_claude_launch(4242, Some(started_at), Some("session-123".to_string()));

        let info = registry.find_claude_launch(4242).expect("launch info");
        assert_eq!(info.started_at, Some(started_at));
        assert_eq!(info.resume_session_id.as_deref(), Some("session-123"));

        registry.remove(4242);
        assert!(registry.find_claude_launch(4242).is_none());
    }
}
