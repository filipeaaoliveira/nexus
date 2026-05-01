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
///
/// When `incoming` carries a concrete session id, entries bound to a
/// *different* session id are excluded so that distinct sessions sharing the
/// same codex PID/cwd (e.g. after `/clear`) are not merged into one another.
/// Placeholder agents without a session id remain enrichable.
pub fn session_indices_by_pid(
    agents: &[Agent],
    incoming: &AgentId,
    pid: u32,
    cwd: &Path,
) -> Vec<usize> {
    let incoming_session_id = incoming.session_id();
    agents
        .iter()
        .enumerate()
        .filter(|(_, a)| {
            a.id().tool() == incoming.tool()
                && a.id().is_session()
                && a.pid() == Some(pid)
                && a.status() != PrimaryStatus::Exited
                && (a.cwd().is_none() || a.cwd() == Some(cwd))
                && session_id_matches(a.id().session_id(), incoming_session_id)
        })
        .map(|(idx, _)| idx)
        .collect()
}

/// Indices of exited session-type agents matching tool + pid + cwd.
///
/// Same session-id discipline as [`session_indices_by_pid`]: distinct sessions
/// sharing a PID are never co-mingled.
pub fn exited_session_indices_by_pid(
    agents: &[Agent],
    incoming: &AgentId,
    pid: u32,
    cwd: &Path,
) -> Vec<usize> {
    let incoming_session_id = incoming.session_id();
    agents
        .iter()
        .enumerate()
        .filter(|(_, a)| {
            a.id().tool() == incoming.tool()
                && a.id().is_session()
                && a.pid() == Some(pid)
                && a.status() == PrimaryStatus::Exited
                && a.cwd() == Some(cwd)
                && session_id_matches(a.id().session_id(), incoming_session_id)
        })
        .map(|(idx, _)| idx)
        .collect()
}

/// A stored agent matches the incoming id when either side is a placeholder
/// without a session id, or when both session ids are equal.
fn session_id_matches(existing: Option<&str>, incoming: Option<&str>) -> bool {
    match (existing, incoming) {
        (Some(existing), Some(incoming)) => existing == incoming,
        _ => true,
    }
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
        let incoming = AgentId::session(AgentTool::Codex, "session-1");
        let indices = session_indices_by_pid(&agents, &incoming, 1, Path::new("/tmp/test"));
        assert_eq!(indices, vec![0]);
    }

    #[test]
    fn session_indices_excludes_agents_with_different_session_id() {
        let agents = vec![
            make_session_agent(AgentTool::Codex, 1, "old-thread"),
            make_session_agent(AgentTool::Codex, 1, "new-thread"),
        ];
        let incoming = AgentId::session(AgentTool::Codex, "new-thread");
        let indices = session_indices_by_pid(&agents, &incoming, 1, Path::new("/tmp/test"));
        assert_eq!(indices, vec![1]);
    }

    #[test]
    fn exited_session_indices_respect_session_id() {
        // The live helper is the one the reducer hits today, but the exited
        // variant shares the filter; locking the semantics here prevents a
        // future caller from regressing the `/clear` discipline.
        let mut agents = vec![
            make_session_agent(AgentTool::Codex, 1, "old-thread"),
            make_session_agent(AgentTool::Codex, 1, "new-thread"),
        ];
        for agent in &mut agents {
            agent.set_status(PrimaryStatus::Exited);
        }
        let incoming = AgentId::session(AgentTool::Codex, "new-thread");
        let indices = exited_session_indices_by_pid(&agents, &incoming, 1, Path::new("/tmp/test"));
        assert_eq!(indices, vec![1]);
    }

    #[test]
    fn session_indices_allow_process_id_to_enrich_any_session_agent() {
        // The process scanner emits `Discovered` with a process-type id.  It
        // should still match the existing codex session agent so enrichment
        // (setting cwd/pid/started_at) works, regardless of the session id.
        let agents = vec![make_session_agent(AgentTool::Codex, 1, "thread-x")];
        let incoming = AgentId::new(AgentTool::Codex, 1);
        let indices = session_indices_by_pid(&agents, &incoming, 1, Path::new("/tmp/test"));
        assert_eq!(indices, vec![0]);
    }

    fn make_session_agent(tool: AgentTool, pid: u32, session_id: &str) -> Agent {
        let now = chrono::Utc::now();
        Agent {
            ident: AgentIdent {
                id: AgentId::session(tool, session_id),
                pid: Some(pid),
                session_id: Some(session_id.to_string()),
                alias: None,
                cwd: Some(PathBuf::from("/tmp/test")),
                branch: None,
                is_worktree: false,
                started_at: now,
            },
            lifecycle: AgentLifecycle {
                status: PrimaryStatus::Running,
                normalized: NormalizedState::new(NormalizedStatus::Unknown, None),
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
}
