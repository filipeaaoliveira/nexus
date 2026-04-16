use chrono::Local;

use super::*;
use crate::event::AgentEvent;

pub struct AppState {
    pub(in crate::app) root: RootState,
    pub(in crate::app) agents: Vec<Agent>,
}

#[cfg(test)]
impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

impl AppState {
    #[cfg(test)]
    pub fn new() -> Self {
        Self::from_root(RootState::default())
    }

    pub fn from_root(root: RootState) -> Self {
        Self {
            root,
            agents: Vec::new(),
        }
    }

    pub fn active_tab(&self) -> Workspace {
        self.root.workspace().active()
    }

    pub fn root(&self) -> &RootState {
        &self.root
    }

    pub fn agents(&self) -> &[Agent] {
        &self.agents
    }

    #[cfg(test)]
    pub fn next_tab(&mut self) {
        self.root.next_workspace();
    }

    #[cfg(test)]
    pub fn previous_tab(&mut self) {
        self.root.previous_workspace();
    }

    pub fn show_help(&self) -> bool {
        matches!(self.root.system().overlay(), Overlay::Help { .. })
    }

    pub fn help_scroll(&self) -> u16 {
        match self.root.system().overlay() {
            Overlay::Help { scroll } => *scroll,
            Overlay::None => 0,
        }
    }

    pub fn handle_event(&mut self, event: AgentEvent) {
        let now = Local::now();
        match event {
            AgentEvent::DetectorHealthChanged {
                detector,
                healthy,
                message,
            } => self.root.record_detector_health(detector, healthy, message),
            AgentEvent::Discovered {
                id,
                pid,
                cwd,
                started_at,
            } => self.handle_discovered(id, pid, cwd, started_at, now),
            AgentEvent::StatusChanged { id, pid, old, new } => {
                self.handle_status_changed(id, pid, old, new, now)
            }
            AgentEvent::ActivityChanged { id, pid, activity } => {
                self.handle_activity_changed(id, pid, activity, now)
            }
            AgentEvent::NormalizedStateChanged { id, pid, state } => {
                self.handle_normalized_state_changed(id, pid, state, now);
            }
            AgentEvent::SubagentSpawned { id, pid, subagent } => {
                self.handle_subagent_spawned(id, pid, subagent, now)
            }
            AgentEvent::SubagentCompleted {
                id,
                pid,
                subagent_name,
            } => self.handle_subagent_completed(id, pid, subagent_name, now),
            AgentEvent::SubagentsUpdated { id, pid, subagents } => {
                self.handle_subagents_updated(id, pid, subagents, now)
            }
            AgentEvent::ToolCallStarted {
                id,
                pid,
                tool_name,
                args_summary,
            } => self.handle_tool_call_started(id, pid, tool_name, args_summary, now),
            AgentEvent::ToolCallCompleted { id, pid } => {
                self.handle_tool_call_completed(id, pid, now)
            }
            AgentEvent::ProcessExited { id, pid } => self.handle_process_exited(id, pid, now),
            AgentEvent::SessionInfo {
                id,
                pid,
                session_id,
                branch,
                is_worktree,
                started_at,
            } => self.handle_session_info(
                id,
                SessionInfoUpdate {
                    pid,
                    session_id,
                    branch,
                    is_worktree,
                    started_at,
                },
                now,
            ),
            AgentEvent::TodosUpdated { id, pid, todos } => {
                self.handle_todos_updated(id, pid, todos, now)
            }
        }
        self.clamp_selection();
    }

    pub fn visible_agents(&self) -> Vec<&Agent> {
        crate::app::projection::visible_agents(&self.agents, self.root.overview())
    }

    #[cfg(test)]
    pub fn count_by_status(&self, status: PrimaryStatus) -> usize {
        crate::app::projection::count_by_status(&self.agents, self.root.overview(), status)
    }

    pub fn primary_detector_issue(&self) -> Option<&str> {
        self.root
            .system()
            .detector_health()
            .values()
            .find(|health| !health.healthy)
            .map(|health| health.message.as_str())
    }
}

#[cfg(test)]
#[path = "core_tests.rs"]
mod tests;
