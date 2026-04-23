//! Unified query helpers for finding agents in the agent list.
//!
//! All functions take `&[Agent]` or `&mut [Agent]` to keep reducers stateless.

use std::path::Path;

use crate::app::agent::Agent;
use nexus_domain::agents::ids::{AgentId, AgentTool};
use nexus_domain::agents::status::PrimaryStatus;

/// Find a single agent by exact AgentId match.
pub fn find_by_id<'a>(agents: &'a mut [Agent], id: &AgentId) -> Option<&'a mut Agent> {
    agents.iter_mut().find(|a| *a.id() == *id)
}

/// Find a single agent by tool + pid (any id type).
pub fn find_by_pid(agents: &mut [Agent], tool: AgentTool, pid: u32) -> Option<&mut Agent> {
    agents
        .iter_mut()
        .find(|a| a.id().tool() == tool && a.pid() == Some(pid))
}

/// Find a process-type agent by tool + pid.
pub fn find_process_agent(agents: &mut [Agent], tool: AgentTool, pid: u32) -> Option<&mut Agent> {
    agents
        .iter_mut()
        .find(|a| a.id().tool() == tool && a.pid() == Some(pid) && a.id().is_process())
}

/// Indices of live session-type agents matching tool + pid + cwd.
pub fn session_indices_by_pid(
    agents: &[Agent],
    tool: AgentTool,
    pid: u32,
    cwd: &Path,
) -> Vec<usize> {
    agents
        .iter()
        .enumerate()
        .filter(|(_, a)| {
            a.id().tool() == tool
                && a.id().is_session()
                && a.pid() == Some(pid)
                && a.status() != PrimaryStatus::Exited
                && (a.cwd().is_none() || a.cwd() == Some(cwd))
        })
        .map(|(idx, _)| idx)
        .collect()
}

/// Indices of exited session-type agents matching tool + pid + cwd.
pub fn exited_session_indices_by_pid(
    agents: &[Agent],
    tool: AgentTool,
    pid: u32,
    cwd: &Path,
) -> Vec<usize> {
    agents
        .iter()
        .enumerate()
        .filter(|(_, a)| {
            a.id().tool() == tool
                && a.id().is_session()
                && a.pid() == Some(pid)
                && a.status() == PrimaryStatus::Exited
                && a.cwd() == Some(cwd)
        })
        .map(|(idx, _)| idx)
        .collect()
}

/// Remove all process-type Codex agents with the given pid.
pub fn remove_redundant_codex_process_agents(agents: &mut Vec<Agent>, pid: Option<u32>) {
    let Some(pid) = pid else {
        return;
    };
    agents.retain(|agent| {
        !(agent.id().tool() == AgentTool::Codex
            && agent.id().is_process()
            && agent.pid() == Some(pid))
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::agent::{AgentDetail, AgentIdent, AgentLifecycle};
    use nexus_domain::agents::status::{NormalizedState, NormalizedStatus};
    use std::collections::VecDeque;
    use std::path::PathBuf;

    fn make_agent(tool: AgentTool, pid: u32, is_session: bool) -> Agent {
        let now = chrono::Utc::now();
        let id = if is_session {
            AgentId::session(tool, format!("session-{pid}"))
        } else {
            AgentId::new(tool, pid)
        };
        Agent {
            ident: AgentIdent {
                id,
                pid: Some(pid),
                session_id: None,
                alias: None,
                cwd: Some(PathBuf::from("/tmp/test")),
                branch: None,
                is_worktree: false,
                started_at: now,
            },
            lifecycle: AgentLifecycle {
                status: PrimaryStatus::Running,
                normalized: NormalizedState::new(NormalizedStatus::Working, None),
                activity: None,
                last_activity: now,
            },
            detail: AgentDetail {
                subagents: Vec::new(),
                recent_events: VecDeque::new(),
                todos: Vec::new(),
            },
        }
    }

    #[test]
    fn find_by_id_returns_matching_agent() {
        let mut agents = vec![
            make_agent(AgentTool::Claude, 1, false),
            make_agent(AgentTool::Codex, 2, false),
        ];
        let id = AgentId::new(AgentTool::Codex, 2);
        assert!(find_by_id(&mut agents, &id).is_some());
    }

    #[test]
    fn find_by_id_returns_none_for_missing() {
        let mut agents = vec![make_agent(AgentTool::Claude, 1, false)];
        let id = AgentId::new(AgentTool::Codex, 99);
        assert!(find_by_id(&mut agents, &id).is_none());
    }

    #[test]
    fn find_process_agent_ignores_session_agents() {
        let mut agents = vec![make_agent(AgentTool::Codex, 1, true)];
        assert!(find_process_agent(&mut agents, AgentTool::Codex, 1).is_none());
    }

    #[test]
    fn session_indices_filters_by_status_and_cwd() {
        let mut agents = vec![
            make_agent(AgentTool::Codex, 1, true),
            make_agent(AgentTool::Codex, 1, true),
        ];
        agents[1].set_status(PrimaryStatus::Exited);
        let indices = session_indices_by_pid(
            &agents,
            AgentTool::Codex,
            1,
            Path::new("/tmp/test"),
        );
        assert_eq!(indices, vec![0]);
    }
}
