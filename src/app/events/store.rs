use super::*;
use std::collections::VecDeque;
use std::path::Path;

impl AppState {
    pub(in crate::app) fn find_agent_mut_by_id(&mut self, id: &AgentId) -> Option<&mut Agent> {
        self.agents.iter_mut().find(|a| *a.id() == *id)
    }

    pub(in crate::app) fn insert_placeholder_agent(
        &mut self,
        id: AgentId,
        pid: Option<u32>,
        now: chrono::DateTime<chrono::Local>,
    ) -> &mut Agent {
        if let Some(agent_idx) = self.agents.iter().position(|agent| *agent.id() == id) {
            return &mut self.agents[agent_idx];
        }

        let session_id = id.session_id().map(str::to_string);
        let normalized = if id.tool() == AgentTool::Codex {
            NormalizedState::new(NormalizedStatus::Unknown, None)
        } else {
            NormalizedState::new(NormalizedStatus::Working, None)
        };
        self.agents.push(Agent {
            ident: AgentIdent {
                id,
                pid,
                session_id,
                alias: None,
                cwd: None,
                branch: None,
                is_worktree: false,
                started_at: now,
            },
            lifecycle: AgentLifecycle {
                status: PrimaryStatus::Running,
                normalized,
                activity: None,
                last_activity: now,
            },
            detail: AgentDetail {
                subagents: Vec::new(),
                recent_events: VecDeque::new(),
                todos: Vec::new(),
            },
        });
        self.agents
            .last_mut()
            .expect("placeholder agent was just inserted")
    }

    pub(in crate::app) fn find_process_agent_mut(
        &mut self,
        tool: AgentTool,
        pid: u32,
    ) -> Option<&mut Agent> {
        self.agents
            .iter_mut()
            .find(|a| a.id().tool() == tool && a.pid() == Some(pid) && a.id().is_process())
    }

    pub(in crate::app) fn find_session_agent_indices_by_pid(
        &self,
        tool: AgentTool,
        pid: u32,
        cwd: &Path,
    ) -> Vec<usize> {
        self.agents
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

    pub(in crate::app) fn find_exited_session_agent_indices_by_pid(
        &self,
        tool: AgentTool,
        pid: u32,
        cwd: &Path,
    ) -> Vec<usize> {
        self.agents
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

    pub(in crate::app) fn find_agent_mut_by_pid(
        &mut self,
        tool: AgentTool,
        pid: u32,
    ) -> Option<&mut Agent> {
        self.agents
            .iter_mut()
            .find(|a| a.id().tool() == tool && a.pid() == Some(pid))
    }

    pub(in crate::app) fn remove_redundant_codex_process_agents(&mut self, pid: Option<u32>) {
        let Some(pid) = pid else {
            return;
        };

        let len_before = self.agents.len();
        self.agents.retain(|agent| {
            !(agent.id().tool() == AgentTool::Codex
                && agent.id().is_process()
                && agent.pid() == Some(pid))
        });

        if self.agents.len() != len_before {
            self.mark_agents_dirty();
            self.clamp_selection();
        }
    }

    pub(in crate::app) fn mark_agent_exited(
        agent: &mut Agent,
        pid: Option<u32>,
        now: chrono::DateTime<chrono::Local>,
    ) {
        agent.ident.pid = pid.or(agent.ident.pid);
        agent.lifecycle.status = PrimaryStatus::Exited;
        agent.lifecycle.activity = None;
        agent.lifecycle.last_activity = now;
        push_event(
            &mut agent.detail.recent_events,
            ActivityEventKind::StatusChanged,
            "Process exited".to_string(),
            now,
        );
    }

    pub(in crate::app) fn apply_session_info(
        agent: &mut Agent,
        pid: Option<u32>,
        session_id: Option<String>,
        branch: Option<String>,
        is_worktree: bool,
        started_at: Option<chrono::DateTime<chrono::Local>>,
        now: chrono::DateTime<chrono::Local>,
    ) {
        agent.ident.pid = pid.or(agent.ident.pid);
        if session_id.is_some() {
            agent.ident.session_id = session_id;
        }
        if branch.is_some() {
            agent.ident.branch = branch;
        }
        agent.ident.is_worktree = is_worktree;
        if let Some(real_start) = started_at {
            if real_start < agent.ident.started_at {
                agent.ident.started_at = real_start;
            }
        }
        agent.lifecycle.last_activity = now;
    }
}
