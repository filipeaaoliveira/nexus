use super::*;

impl AppState {
    pub(in crate::app) fn handle_discovered(
        &mut self,
        id: AgentId,
        pid: Option<u32>,
        cwd: PathBuf,
        real_start: Option<chrono::DateTime<chrono::Local>>,
        now: chrono::DateTime<chrono::Local>,
    ) {
        let pid = pid.or_else(|| id.pid());
        let preserve_codex_session_status = id.tool() == AgentTool::Codex && id.is_process();
        if let Some(agent_idx) = self.agents.iter().position(|a| *a.id() == id) {
            let absorbed_pid = {
                let agent = &mut self.agents[agent_idx];
                agent.set_cwd(cwd);
                agent.merge_pid(pid);
                if !(preserve_codex_session_status && agent.id().is_session()) {
                    agent.set_status(PrimaryStatus::Running);
                }
                if let Some(real_start) = real_start {
                    agent.clamp_started_at(real_start);
                }
                agent.touch(now);
                agent.pid()
            };
            self.mark_agents_dirty();
            if id.tool() == AgentTool::Codex && id.is_session() {
                self.remove_redundant_codex_process_agents(absorbed_pid);
            }
            self.clamp_selection();
            return;
        }

        let session_id = id.session_id().map(str::to_string);
        if id.tool() == AgentTool::Codex {
            if let Some(pid) = pid {
                let matching_agent_indices =
                    self.find_session_agent_indices_by_pid(id.tool(), pid, &cwd);
                if matching_agent_indices.len() > 1 {
                    for agent_idx in matching_agent_indices {
                        let agent = &mut self.agents[agent_idx];
                        agent.set_cwd(cwd.clone());
                        agent.merge_pid(Some(pid));
                        if !preserve_codex_session_status {
                            agent.set_status(PrimaryStatus::Running);
                        }
                        if let Some(real_start) = real_start {
                            agent.clamp_started_at(real_start);
                        }
                        agent.touch(now);
                    }
                    self.mark_agents_dirty();
                    self.clamp_selection();
                    return;
                }
                if let Some(agent_idx) = matching_agent_indices.into_iter().next() {
                    let agent = &mut self.agents[agent_idx];
                    agent.set_cwd(cwd);
                    agent.merge_pid(Some(pid));
                    if !preserve_codex_session_status {
                        agent.set_status(PrimaryStatus::Running);
                    }
                    if let Some(real_start) = real_start {
                        agent.clamp_started_at(real_start);
                    }
                    agent.touch(now);
                    self.mark_agents_dirty();
                    self.clamp_selection();
                    return;
                }

                if id.is_process() {
                    let exited_agent_indices =
                        self.find_exited_session_agent_indices_by_pid(id.tool(), pid, &cwd);
                    if !exited_agent_indices.is_empty() {
                        for agent_idx in exited_agent_indices {
                            let agent = &mut self.agents[agent_idx];
                            agent.set_cwd(cwd.clone());
                            agent.merge_pid(Some(pid));
                            agent.set_status(PrimaryStatus::Running);
                            if let Some(real_start) = real_start {
                                agent.clamp_started_at(real_start);
                            }
                            agent.touch(now);
                        }
                        self.mark_agents_dirty();
                        self.clamp_selection();
                        return;
                    }
                }
            }
        }

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
                cwd: Some(cwd),
                branch: None,
                is_worktree: false,
                started_at: real_start.unwrap_or(now),
            },
            lifecycle: AgentLifecycle {
                status: PrimaryStatus::Running,
                normalized,
                activity: None,
                last_activity: now,
            },
            detail: AgentDetail {
                subagents: Vec::new(),
                recent_events: std::collections::VecDeque::new(),
                todos: Vec::new(),
            },
        });
        self.mark_agents_dirty();
        if self.root.overview.selected_index.is_none() {
            self.root.overview.selected_index = Some(0);
        }
        self.clamp_selection();
    }

    pub(in crate::app) fn handle_session_info(
        &mut self,
        id: AgentId,
        update: SessionInfoUpdate,
        now: chrono::DateTime<chrono::Local>,
    ) {
        let pid = update.pid.or_else(|| id.pid());
        if id.is_session() {
            if let Some(agent) = self.find_agent_mut_by_id(&id) {
                let absorbed_pid = {
                    Self::apply_session_info(
                        agent,
                        pid,
                        update.session_id.clone(),
                        update.branch.clone(),
                        update.is_worktree,
                        update.started_at,
                        now,
                    );
                    agent.pid()
                };
                self.mark_agents_dirty();
                self.remove_redundant_codex_process_agents(absorbed_pid);
                self.clamp_selection();
                return;
            }

            if let Some(pid) = pid {
                if let Some(agent) = self.find_process_agent_mut(id.tool(), pid) {
                    agent.set_id(id.clone());
                    Self::apply_session_info(
                        agent,
                        Some(pid),
                        update.session_id.clone(),
                        update.branch.clone(),
                        update.is_worktree,
                        update.started_at,
                        now,
                    );
                    self.mark_agents_dirty();
                    self.remove_redundant_codex_process_agents(Some(pid));
                    self.clamp_selection();
                    return;
                }
            }

            let agent = self.insert_placeholder_agent(id, pid, now);
            Self::apply_session_info(
                agent,
                pid,
                update.session_id,
                update.branch,
                update.is_worktree,
                update.started_at,
                now,
            );
            self.mark_agents_dirty();
            self.remove_redundant_codex_process_agents(pid);
            self.clamp_selection();
            return;
        }

        if let Some(agent) = self.find_agent_mut_by_id(&id) {
            Self::apply_session_info(
                agent,
                pid,
                update.session_id.clone(),
                update.branch.clone(),
                update.is_worktree,
                update.started_at,
                now,
            );
        } else {
            let agent = self.insert_placeholder_agent(id, pid, now);
            Self::apply_session_info(
                agent,
                pid,
                update.session_id,
                update.branch,
                update.is_worktree,
                update.started_at,
                now,
            );
        }
        self.mark_agents_dirty();
        self.clamp_selection();
    }
}
