use std::collections::HashMap;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Local};
use tokio::sync::{mpsc, oneshot};

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

/// All mutable state owned by the registry actor.
#[derive(Debug, Clone, Default)]
struct RegistryState {
    agents: HashMap<AgentKey, PidList>,
    codex_thread_pids: HashMap<String, u32>,
    codex_launches: HashMap<u32, CodexLaunchInfo>,
    claude_launches: HashMap<u32, ClaudeLaunchInfo>,
    opencode_launches: HashMap<u32, OpenCodeLaunchInfo>,
}

/// A point-in-time clone of the registry state.
///
/// Provides the same read-only query methods as the old `AgentRegistry` but
/// operates on owned data. Created via `RegistryHandle::snapshot()` for use
/// in synchronous contexts (e.g. inside `spawn_blocking`).
#[derive(Debug, Clone, Default)]
pub struct RegistrySnapshot {
    state: RegistryState,
}

#[allow(dead_code)]
impl RegistrySnapshot {
    /// Create a new empty snapshot. Primarily useful for tests.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register an agent directly into the snapshot (for testing).
    pub fn register(&mut self, tool: AgentTool, cwd: PathBuf, pid: u32) {
        let entry = self.state.agents.entry((tool, cwd)).or_default();
        if tool == AgentTool::Codex {
            if !entry.contains(&pid) {
                entry.push(pid);
            }
        } else {
            entry.clear();
            entry.push(pid);
        }
    }

    /// Register a Codex thread binding directly (for testing).
    pub fn register_codex_thread(&mut self, thread_id: String, pid: u32) {
        self.state.codex_thread_pids.insert(thread_id, pid);
    }

    /// Register Codex launch metadata directly (for testing).
    pub fn register_codex_launch(&mut self, pid: u32, started_at: Option<DateTime<Local>>) {
        self.state
            .codex_launches
            .insert(pid, CodexLaunchInfo { started_at });
    }

    /// Register Claude launch metadata directly (for testing).
    pub fn register_claude_launch(
        &mut self,
        pid: u32,
        started_at: Option<DateTime<Local>>,
        resume_session_id: Option<String>,
    ) {
        self.state.claude_launches.insert(
            pid,
            ClaudeLaunchInfo {
                started_at,
                resume_session_id,
            },
        );
    }

    /// Register OpenCode launch metadata directly (for testing).
    pub fn register_opencode_launch(&mut self, pid: u32, started_at: Option<DateTime<Local>>) {
        self.state
            .opencode_launches
            .insert(pid, OpenCodeLaunchInfo { started_at });
    }

    /// Remove an agent by pid directly (for testing).
    pub fn remove(&mut self, pid: u32) {
        self.state.agents.retain(|_, pids| {
            pids.retain(|current_pid| *current_pid != pid);
            !pids.is_empty()
        });
        self.state
            .codex_thread_pids
            .retain(|_, current_pid| *current_pid != pid);
        self.state.codex_launches.remove(&pid);
        self.state.claude_launches.remove(&pid);
        self.state.opencode_launches.remove(&pid);
    }

    pub fn find_pid(&self, tool: AgentTool, cwd: &Path) -> Option<u32> {
        find_pid_in_state(&self.state, tool, cwd)
    }

    pub fn find_pid_prefix(&self, tool: AgentTool, cwd: &Path) -> Option<u32> {
        find_pid_prefix_in_state(&self.state, tool, cwd)
    }

    pub fn find_pids(&self, tool: AgentTool, cwd: &Path) -> Vec<u32> {
        self.state
            .agents
            .get(&(tool, cwd.to_path_buf()))
            .cloned()
            .unwrap_or_default()
    }

    pub fn find_codex_pid_for_thread(&self, thread_id: &str) -> Option<u32> {
        self.state.codex_thread_pids.get(thread_id).copied()
    }

    pub fn find_codex_launch(&self, pid: u32) -> Option<CodexLaunchInfo> {
        self.state.codex_launches.get(&pid).cloned()
    }

    pub fn find_claude_launch(&self, pid: u32) -> Option<ClaudeLaunchInfo> {
        self.state.claude_launches.get(&pid).cloned()
    }

    pub fn find_opencode_launch(&self, pid: u32) -> Option<OpenCodeLaunchInfo> {
        self.state.opencode_launches.get(&pid).cloned()
    }

    pub fn agents_for_tool(&self, tool: AgentTool) -> Vec<(PathBuf, u32)> {
        agents_for_tool_in_state(&self.state, tool)
    }
}

// ── Commands ─────────────────────────────────────────────────────

/// Messages processed by the registry actor.
#[allow(dead_code)]
enum RegistryCommand {
    // Write commands (fire-and-forget)
    Register {
        tool: AgentTool,
        cwd: PathBuf,
        pid: u32,
    },
    RegisterCodexThread {
        thread_id: String,
        pid: u32,
    },
    RegisterCodexLaunch {
        pid: u32,
        started_at: Option<DateTime<Local>>,
    },
    RegisterClaudeLaunch {
        pid: u32,
        started_at: Option<DateTime<Local>>,
        resume_session_id: Option<String>,
    },
    RegisterOpenCodeLaunch {
        pid: u32,
        started_at: Option<DateTime<Local>>,
    },
    Remove {
        pid: u32,
    },

    // Read commands (request-response via oneshot)
    FindPid {
        tool: AgentTool,
        cwd: PathBuf,
        reply: oneshot::Sender<Option<u32>>,
    },
    FindPidPrefix {
        tool: AgentTool,
        cwd: PathBuf,
        reply: oneshot::Sender<Option<u32>>,
    },
    FindPids {
        tool: AgentTool,
        cwd: PathBuf,
        reply: oneshot::Sender<Vec<u32>>,
    },
    FindCodexPidForThread {
        thread_id: String,
        reply: oneshot::Sender<Option<u32>>,
    },
    FindCodexLaunch {
        pid: u32,
        reply: oneshot::Sender<Option<CodexLaunchInfo>>,
    },
    FindClaudeLaunch {
        pid: u32,
        reply: oneshot::Sender<Option<ClaudeLaunchInfo>>,
    },
    FindOpenCodeLaunch {
        pid: u32,
        reply: oneshot::Sender<Option<OpenCodeLaunchInfo>>,
    },
    AgentsForTool {
        tool: AgentTool,
        reply: oneshot::Sender<Vec<(PathBuf, u32)>>,
    },

    // Snapshot: returns a full clone for sync callers
    Snapshot {
        reply: oneshot::Sender<RegistrySnapshot>,
    },
}

// ── Actor ────────────────────────────────────────────────────────

/// Background task that owns registry state and processes commands sequentially.
async fn registry_actor(mut rx: mpsc::Receiver<RegistryCommand>) {
    let mut state = RegistryState::default();

    while let Some(cmd) = rx.recv().await {
        match cmd {
            RegistryCommand::Register { tool, cwd, pid } => {
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
            RegistryCommand::RegisterCodexThread { thread_id, pid } => {
                state.codex_thread_pids.insert(thread_id, pid);
            }
            RegistryCommand::RegisterCodexLaunch { pid, started_at } => {
                state
                    .codex_launches
                    .insert(pid, CodexLaunchInfo { started_at });
            }
            RegistryCommand::RegisterClaudeLaunch {
                pid,
                started_at,
                resume_session_id,
            } => {
                state.claude_launches.insert(
                    pid,
                    ClaudeLaunchInfo {
                        started_at,
                        resume_session_id,
                    },
                );
            }
            RegistryCommand::RegisterOpenCodeLaunch { pid, started_at } => {
                state
                    .opencode_launches
                    .insert(pid, OpenCodeLaunchInfo { started_at });
            }
            RegistryCommand::Remove { pid } => {
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

            // Read commands
            RegistryCommand::FindPid { tool, cwd, reply } => {
                let result = find_pid_in_state(&state, tool, &cwd);
                let _ = reply.send(result);
            }
            RegistryCommand::FindPidPrefix { tool, cwd, reply } => {
                let result = find_pid_prefix_in_state(&state, tool, &cwd);
                let _ = reply.send(result);
            }
            RegistryCommand::FindPids { tool, cwd, reply } => {
                let result = state.agents.get(&(tool, cwd)).cloned().unwrap_or_default();
                let _ = reply.send(result);
            }
            RegistryCommand::FindCodexPidForThread { thread_id, reply } => {
                let result = state.codex_thread_pids.get(&thread_id).copied();
                let _ = reply.send(result);
            }
            RegistryCommand::FindCodexLaunch { pid, reply } => {
                let result = state.codex_launches.get(&pid).cloned();
                let _ = reply.send(result);
            }
            RegistryCommand::FindClaudeLaunch { pid, reply } => {
                let result = state.claude_launches.get(&pid).cloned();
                let _ = reply.send(result);
            }
            RegistryCommand::FindOpenCodeLaunch { pid, reply } => {
                let result = state.opencode_launches.get(&pid).cloned();
                let _ = reply.send(result);
            }
            RegistryCommand::AgentsForTool { tool, reply } => {
                let result = agents_for_tool_in_state(&state, tool);
                let _ = reply.send(result);
            }
            RegistryCommand::Snapshot { reply } => {
                let _ = reply.send(RegistrySnapshot {
                    state: state.clone(),
                });
            }
        }
    }
}

fn find_pid_in_state(state: &RegistryState, tool: AgentTool, cwd: &Path) -> Option<u32> {
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

fn find_pid_prefix_in_state(state: &RegistryState, tool: AgentTool, cwd: &Path) -> Option<u32> {
    if let Some(pids) = state.agents.get(&(tool, cwd.to_path_buf())) {
        return pids.first().copied();
    }

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

fn agents_for_tool_in_state(state: &RegistryState, tool: AgentTool) -> Vec<(PathBuf, u32)> {
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

// ── Handle ───────────────────────────────────────────────────────

/// Clone-friendly handle to the registry actor.
///
/// Wraps an `mpsc::Sender<RegistryCommand>`. Write methods are fire-and-forget;
/// read methods send a command with a oneshot reply and await the response.
#[derive(Debug, Clone)]
pub struct RegistryHandle {
    tx: mpsc::Sender<RegistryCommand>,
}

#[allow(dead_code)]
impl RegistryHandle {
    /// Spawn the registry actor and return a handle to it.
    pub fn spawn() -> Self {
        let (tx, rx) = mpsc::channel(256);
        tokio::spawn(registry_actor(rx));
        Self { tx }
    }

    // ── Write methods (fire-and-forget) ──────────────────────────

    pub fn register(&self, tool: AgentTool, cwd: PathBuf, pid: u32) {
        if self
            .tx
            .try_send(RegistryCommand::Register { tool, cwd, pid })
            .is_err()
        {
            tracing::warn!("registry channel full, dropped command");
        }
    }

    pub fn register_codex_thread(&self, thread_id: String, pid: u32) {
        if self
            .tx
            .try_send(RegistryCommand::RegisterCodexThread { thread_id, pid })
            .is_err()
        {
            tracing::warn!("registry channel full, dropped command");
        }
    }

    pub fn register_codex_launch(&self, pid: u32, started_at: Option<DateTime<Local>>) {
        if self
            .tx
            .try_send(RegistryCommand::RegisterCodexLaunch { pid, started_at })
            .is_err()
        {
            tracing::warn!("registry channel full, dropped command");
        }
    }

    pub fn register_claude_launch(
        &self,
        pid: u32,
        started_at: Option<DateTime<Local>>,
        resume_session_id: Option<String>,
    ) {
        if self
            .tx
            .try_send(RegistryCommand::RegisterClaudeLaunch {
                pid,
                started_at,
                resume_session_id,
            })
            .is_err()
        {
            tracing::warn!("registry channel full, dropped command");
        }
    }

    pub fn register_opencode_launch(&self, pid: u32, started_at: Option<DateTime<Local>>) {
        if self
            .tx
            .try_send(RegistryCommand::RegisterOpenCodeLaunch { pid, started_at })
            .is_err()
        {
            tracing::warn!("registry channel full, dropped command");
        }
    }

    pub fn remove(&self, pid: u32) {
        if self.tx.try_send(RegistryCommand::Remove { pid }).is_err() {
            tracing::warn!("registry channel full, dropped command");
        }
    }

    // ── Read methods (request-response) ──────────────────────────

    pub async fn find_pid(&self, tool: AgentTool, cwd: &Path) -> Option<u32> {
        let (reply, rx) = oneshot::channel();
        let _ = self
            .tx
            .send(RegistryCommand::FindPid {
                tool,
                cwd: cwd.to_path_buf(),
                reply,
            })
            .await;
        rx.await.ok().flatten()
    }

    pub async fn find_pid_prefix(&self, tool: AgentTool, cwd: &Path) -> Option<u32> {
        let (reply, rx) = oneshot::channel();
        let _ = self
            .tx
            .send(RegistryCommand::FindPidPrefix {
                tool,
                cwd: cwd.to_path_buf(),
                reply,
            })
            .await;
        rx.await.ok().flatten()
    }

    pub async fn find_pids(&self, tool: AgentTool, cwd: &Path) -> Vec<u32> {
        let (reply, rx) = oneshot::channel();
        let _ = self
            .tx
            .send(RegistryCommand::FindPids {
                tool,
                cwd: cwd.to_path_buf(),
                reply,
            })
            .await;
        rx.await.ok().unwrap_or_default()
    }

    pub async fn find_codex_pid_for_thread(&self, thread_id: &str) -> Option<u32> {
        let (reply, rx) = oneshot::channel();
        let _ = self
            .tx
            .send(RegistryCommand::FindCodexPidForThread {
                thread_id: thread_id.to_string(),
                reply,
            })
            .await;
        rx.await.ok().flatten()
    }

    pub async fn find_codex_launch(&self, pid: u32) -> Option<CodexLaunchInfo> {
        let (reply, rx) = oneshot::channel();
        let _ = self
            .tx
            .send(RegistryCommand::FindCodexLaunch { pid, reply })
            .await;
        rx.await.ok().flatten()
    }

    pub async fn find_claude_launch(&self, pid: u32) -> Option<ClaudeLaunchInfo> {
        let (reply, rx) = oneshot::channel();
        let _ = self
            .tx
            .send(RegistryCommand::FindClaudeLaunch { pid, reply })
            .await;
        rx.await.ok().flatten()
    }

    pub async fn find_opencode_launch(&self, pid: u32) -> Option<OpenCodeLaunchInfo> {
        let (reply, rx) = oneshot::channel();
        let _ = self
            .tx
            .send(RegistryCommand::FindOpenCodeLaunch { pid, reply })
            .await;
        rx.await.ok().flatten()
    }

    pub async fn agents_for_tool(&self, tool: AgentTool) -> Vec<(PathBuf, u32)> {
        let (reply, rx) = oneshot::channel();
        let _ = self
            .tx
            .send(RegistryCommand::AgentsForTool { tool, reply })
            .await;
        rx.await.ok().unwrap_or_default()
    }

    /// Get a point-in-time snapshot of the full registry state.
    ///
    /// Useful for synchronous callers (e.g. inside `spawn_blocking`) that need
    /// to issue multiple read queries without holding an async context.
    pub async fn snapshot(&self) -> RegistrySnapshot {
        let (reply, rx) = oneshot::channel();
        let _ = self.tx.send(RegistryCommand::Snapshot { reply }).await;
        rx.await.ok().unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Local;
    use std::path::PathBuf;

    #[tokio::test]
    async fn test_opencode_launch_register_and_find() {
        let handle = RegistryHandle::spawn();
        let now = Local::now();
        handle.register_opencode_launch(99, Some(now));
        let info = handle
            .find_opencode_launch(99)
            .await
            .expect("should find launch info");
        assert_eq!(info.started_at, Some(now));
    }

    #[tokio::test]
    async fn test_opencode_launch_removed_on_remove() {
        let handle = RegistryHandle::spawn();
        let now = Local::now();
        handle.register_opencode_launch(99, Some(now));
        // Ensure registration is processed before remove
        let _ = handle.find_opencode_launch(99).await;
        handle.remove(99);
        // Ensure remove is processed
        assert!(handle.find_opencode_launch(99).await.is_none());
    }

    #[tokio::test]
    async fn test_opencode_launch_none_started_at() {
        let handle = RegistryHandle::spawn();
        handle.register_opencode_launch(100, None);
        let info = handle.find_opencode_launch(100).await.expect("should find");
        assert!(info.started_at.is_none());
    }

    #[tokio::test]
    async fn test_codex_registry_returns_none_for_ambiguous_cwd() {
        let handle = RegistryHandle::spawn();
        let cwd = PathBuf::from("/tmp/project");

        handle.register(AgentTool::Codex, cwd.clone(), 101);
        handle.register(AgentTool::Codex, cwd.clone(), 202);

        assert_eq!(handle.find_pid(AgentTool::Codex, &cwd).await, None);
        assert_eq!(
            handle.find_pids(AgentTool::Codex, &cwd).await,
            vec![101, 202]
        );
        assert_eq!(
            handle.agents_for_tool(AgentTool::Codex).await,
            vec![(cwd, 101), (PathBuf::from("/tmp/project"), 202)]
        );
    }

    #[tokio::test]
    async fn test_codex_registry_returns_pid_for_single_cwd_candidate() {
        let handle = RegistryHandle::spawn();
        let cwd = PathBuf::from("/tmp/project");

        handle.register(AgentTool::Codex, cwd.clone(), 101);

        assert_eq!(handle.find_pid(AgentTool::Codex, &cwd).await, Some(101));
        assert_eq!(handle.find_pids(AgentTool::Codex, &cwd).await, vec![101]);
        assert_eq!(
            handle.agents_for_tool(AgentTool::Codex).await,
            vec![(cwd, 101)]
        );
    }

    #[tokio::test]
    async fn test_non_codex_registry_still_overwrites_pid_for_same_cwd() {
        let handle = RegistryHandle::spawn();
        let cwd = PathBuf::from("/tmp/project");

        handle.register(AgentTool::Claude, cwd.clone(), 303);
        handle.register(AgentTool::Claude, cwd.clone(), 404);

        assert_eq!(handle.find_pid(AgentTool::Claude, &cwd).await, Some(404));
        assert_eq!(handle.find_pids(AgentTool::Claude, &cwd).await, vec![404]);
        assert_eq!(
            handle.agents_for_tool(AgentTool::Claude).await,
            vec![(cwd, 404)]
        );
    }

    #[tokio::test]
    async fn test_codex_remove_only_drops_matching_pid() {
        let handle = RegistryHandle::spawn();
        let cwd = PathBuf::from("/tmp/project");

        handle.register(AgentTool::Codex, cwd.clone(), 101);
        handle.register(AgentTool::Codex, cwd.clone(), 202);
        // Ensure registrations are processed
        let _ = handle.find_pids(AgentTool::Codex, &cwd).await;
        handle.remove(101);

        assert_eq!(handle.find_pid(AgentTool::Codex, &cwd).await, Some(202));
        assert_eq!(handle.find_pids(AgentTool::Codex, &cwd).await, vec![202]);
        assert_eq!(
            handle.agents_for_tool(AgentTool::Codex).await,
            vec![(cwd, 202)]
        );
    }

    #[tokio::test]
    async fn test_codex_registry_tracks_exact_thread_pid_binding() {
        let handle = RegistryHandle::spawn();
        let cwd = PathBuf::from("/tmp/project");

        handle.register(AgentTool::Codex, cwd, 15642);
        handle.register_codex_thread("019d4ffb-ca93-7c83-9cb5-72695ed739e0".to_string(), 15642);

        assert_eq!(
            handle
                .find_codex_pid_for_thread("019d4ffb-ca93-7c83-9cb5-72695ed739e0")
                .await,
            Some(15642)
        );

        handle.remove(15642);

        assert_eq!(
            handle
                .find_codex_pid_for_thread("019d4ffb-ca93-7c83-9cb5-72695ed739e0")
                .await,
            None
        );
    }

    #[tokio::test]
    async fn register_codex_launch() {
        let handle = RegistryHandle::spawn();
        let started_at = Local::now();

        handle.register_codex_launch(4242, Some(started_at));

        let info = handle.find_codex_launch(4242).await.expect("launch info");
        assert_eq!(info.started_at, Some(started_at));

        handle.remove(4242);
        assert!(handle.find_codex_launch(4242).await.is_none());
    }

    #[tokio::test]
    async fn test_registry_tracks_claude_launch_metadata() {
        let handle = RegistryHandle::spawn();
        let started_at = Local::now();

        handle.register_claude_launch(4242, Some(started_at), Some("session-123".to_string()));

        let info = handle.find_claude_launch(4242).await.expect("launch info");
        assert_eq!(info.started_at, Some(started_at));
        assert_eq!(info.resume_session_id.as_deref(), Some("session-123"));

        handle.remove(4242);
        assert!(handle.find_claude_launch(4242).await.is_none());
    }

    #[tokio::test]
    async fn test_snapshot_returns_consistent_state() {
        let handle = RegistryHandle::spawn();
        let cwd = PathBuf::from("/tmp/project");

        handle.register(AgentTool::Codex, cwd.clone(), 101);
        handle.register(AgentTool::Codex, cwd.clone(), 202);
        handle.register_codex_thread("thread-abc".to_string(), 101);

        let snap = handle.snapshot().await;
        assert_eq!(snap.find_pids(AgentTool::Codex, &cwd), vec![101, 202]);
        assert_eq!(snap.find_codex_pid_for_thread("thread-abc"), Some(101));
    }

    #[tokio::test]
    async fn test_concurrent_sends_do_not_deadlock() {
        let handle = RegistryHandle::spawn();
        let cwd = PathBuf::from("/tmp/project");

        let mut tasks = Vec::new();
        for i in 0..10_u32 {
            let h = handle.clone();
            let c = cwd.clone();
            tasks.push(tokio::spawn(async move {
                h.register(AgentTool::Claude, c, 1000 + i);
            }));
        }

        for task in tasks {
            task.await.unwrap();
        }

        // The last registration wins for non-Codex tools (single-PID override).
        let pid = handle.find_pid(AgentTool::Claude, &cwd).await;
        assert!(pid.is_some(), "should have exactly one PID registered");
    }

    #[tokio::test]
    async fn test_sequential_codex_sends_arrive_in_order() {
        let handle = RegistryHandle::spawn();
        let cwd = PathBuf::from("/tmp/project");

        // Send registrations sequentially via the same handle to guarantee order
        for i in 0..5_u32 {
            handle.register(AgentTool::Codex, cwd.clone(), 500 + i);
        }

        let pids = handle.find_pids(AgentTool::Codex, &cwd).await;
        assert_eq!(pids, vec![500, 501, 502, 503, 504]);
    }
}
