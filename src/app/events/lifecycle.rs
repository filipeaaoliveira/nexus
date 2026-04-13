use super::*;

impl AppState {
    pub(in crate::app) fn handle_process_exited(
        &mut self,
        id: AgentId,
        pid: Option<u32>,
        now: chrono::DateTime<chrono::Local>,
    ) {
        if (id.tool() == AgentTool::Claude || id.tool() == AgentTool::OpenCode) && id.is_process() {
            let pid = pid.or_else(|| id.pid());
            if let Some(idx) = self.agents.iter().position(|a| a.id == id) {
                self.agents.remove(idx);
                self.mark_agents_dirty();
                self.clamp_selection();
                return;
            }
            if let Some(pid) = pid {
                let indices: Vec<usize> = self
                    .agents
                    .iter()
                    .enumerate()
                    .filter(|(_, a)| a.id.tool() == id.tool() && a.pid == Some(pid))
                    .map(|(idx, _)| idx)
                    .collect();
                for idx in indices.into_iter().rev() {
                    self.agents.remove(idx);
                }
                self.mark_agents_dirty();
                self.clamp_selection();
            }
            return;
        }

        if id.tool() == AgentTool::Codex && id.is_process() {
            let pid = pid.or_else(|| id.pid());

            if let Some(pid) = pid {
                let matching_indices: Vec<usize> = self
                    .agents
                    .iter()
                    .enumerate()
                    .filter(|(_, a)| a.id.tool() == AgentTool::Codex && a.pid == Some(pid))
                    .map(|(idx, _)| idx)
                    .collect();

                if !matching_indices.is_empty() {
                    for agent_idx in matching_indices.into_iter().rev() {
                        if self.agents[agent_idx].id.is_process() {
                            self.agents.remove(agent_idx);
                        } else {
                            let agent = &mut self.agents[agent_idx];
                            agent.status = PrimaryStatus::Exited;
                            agent.activity = None;
                            agent.last_activity = now;
                            push_event(
                                &mut agent.recent_events,
                                ActivityEventKind::StatusChanged,
                                "Process exited".to_string(),
                                now,
                            );
                        }
                    }
                    self.mark_agents_dirty();
                    self.clamp_selection();
                    return;
                }
            }
        }

        if let Some(agent) = self.find_agent_mut_by_id(&id) {
            Self::mark_agent_exited(agent, pid, now);
            self.mark_agents_dirty();
        } else if let Some(pid) = pid {
            if id.tool() == AgentTool::Codex {
                let matching_indices: Vec<usize> = self
                    .agents
                    .iter()
                    .enumerate()
                    .filter(|(_, a)| a.id.tool() == id.tool() && a.pid == Some(pid))
                    .map(|(idx, _)| idx)
                    .collect();

                if !matching_indices.is_empty() {
                    for agent_idx in matching_indices {
                        Self::mark_agent_exited(&mut self.agents[agent_idx], Some(pid), now);
                    }
                    self.mark_agents_dirty();
                    self.clamp_selection();
                    return;
                }
            }

            if let Some(agent) = self.find_agent_mut_by_pid(id.tool(), pid) {
                Self::mark_agent_exited(agent, Some(pid), now);
                self.mark_agents_dirty();
            }
        }
        self.clamp_selection();
    }
}
