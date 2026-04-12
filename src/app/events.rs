use super::*;
use std::collections::VecDeque;

impl AppState {
    pub(super) fn handle_detector_health_changed(
        &mut self,
        detector: String,
        healthy: bool,
        message: String,
    ) {
        if healthy {
            self.detector_issues.remove(&detector);
        } else {
            self.detector_issues.insert(detector, message);
        }
    }

    pub(super) fn find_agent_mut_by_id(&mut self, id: &AgentId) -> Option<&mut Agent> {
        self.agents.iter_mut().find(|a| a.id == *id)
    }

    pub(super) fn insert_placeholder_agent(
        &mut self,
        id: AgentId,
        pid: Option<u32>,
        now: chrono::DateTime<chrono::Local>,
    ) -> &mut Agent {
        if let Some(agent_idx) = self.agents.iter().position(|agent| agent.id == id) {
            return &mut self.agents[agent_idx];
        }

        let session_id = id.session_id().map(str::to_string);
        let normalized = if id.tool() == AgentTool::Codex {
            NormalizedState::new(NormalizedStatus::Unknown, None)
        } else {
            NormalizedState::new(NormalizedStatus::Working, None)
        };
        self.agents.push(Agent {
            id,
            pid,
            status: PrimaryStatus::Running,
            normalized,
            activity: None,
            cwd: PathBuf::new(),
            branch: None,
            is_worktree: false,
            started_at: now,
            last_activity: now,
            subagents: Vec::new(),
            recent_events: VecDeque::new(),
            session_id,
            alias: None,
            todos: Vec::new(),
        });
        self.agents
            .last_mut()
            .expect("placeholder agent was just inserted")
    }

    pub(super) fn find_process_agent_mut(
        &mut self,
        tool: AgentTool,
        pid: u32,
    ) -> Option<&mut Agent> {
        self.agents
            .iter_mut()
            .find(|a| a.id.tool() == tool && a.pid == Some(pid) && a.id.is_process())
    }

    pub(super) fn find_session_agent_indices_by_pid(
        &self,
        tool: AgentTool,
        pid: u32,
        cwd: &Path,
    ) -> Vec<usize> {
        self.agents
            .iter()
            .enumerate()
            .filter(|(_, a)| {
                a.id.tool() == tool
                    && a.id.is_session()
                    && a.pid == Some(pid)
                    && a.status != PrimaryStatus::Exited
                    && (a.cwd.as_os_str().is_empty() || a.cwd == cwd)
            })
            .map(|(idx, _)| idx)
            .collect()
    }

    pub(super) fn find_exited_session_agent_indices_by_pid(
        &self,
        tool: AgentTool,
        pid: u32,
        cwd: &Path,
    ) -> Vec<usize> {
        self.agents
            .iter()
            .enumerate()
            .filter(|(_, a)| {
                a.id.tool() == tool
                    && a.id.is_session()
                    && a.pid == Some(pid)
                    && a.status == PrimaryStatus::Exited
                    && a.cwd == cwd
            })
            .map(|(idx, _)| idx)
            .collect()
    }

    pub(super) fn find_agent_mut_by_pid(
        &mut self,
        tool: AgentTool,
        pid: u32,
    ) -> Option<&mut Agent> {
        self.agents
            .iter_mut()
            .find(|a| a.id.tool() == tool && a.pid == Some(pid))
    }

    pub(super) fn remove_redundant_codex_process_agents(&mut self, pid: Option<u32>) {
        let Some(pid) = pid else {
            return;
        };

        let len_before = self.agents.len();
        self.agents.retain(|agent| {
            !(agent.id.tool() == AgentTool::Codex
                && agent.id.is_process()
                && agent.pid == Some(pid))
        });

        if self.agents.len() != len_before {
            self.mark_agents_dirty();
            self.clamp_selection();
        }
    }

    pub(super) fn handle_discovered(
        &mut self,
        id: AgentId,
        pid: Option<u32>,
        cwd: PathBuf,
        real_start: Option<chrono::DateTime<chrono::Local>>,
        now: chrono::DateTime<chrono::Local>,
    ) {
        let pid = pid.or_else(|| id.pid());
        let preserve_codex_session_status = id.tool() == AgentTool::Codex && id.is_process();
        if let Some(agent_idx) = self.agents.iter().position(|a| a.id == id) {
            let absorbed_pid = {
                let agent = &mut self.agents[agent_idx];
                agent.cwd = cwd;
                agent.pid = pid.or(agent.pid);
                if !(preserve_codex_session_status && agent.id.is_session()) {
                    agent.status = PrimaryStatus::Running;
                }
                if let Some(real_start) = real_start {
                    if real_start < agent.started_at {
                        agent.started_at = real_start;
                    }
                }
                agent.last_activity = now;
                agent.pid
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
                        agent.cwd = cwd.clone();
                        agent.pid = Some(pid);
                        if !preserve_codex_session_status {
                            agent.status = PrimaryStatus::Running;
                        }
                        if let Some(real_start) = real_start {
                            if real_start < agent.started_at {
                                agent.started_at = real_start;
                            }
                        }
                        agent.last_activity = now;
                    }
                    self.mark_agents_dirty();
                    self.clamp_selection();
                    return;
                }
                if let Some(agent_idx) = matching_agent_indices.into_iter().next() {
                    let agent = &mut self.agents[agent_idx];
                    agent.cwd = cwd;
                    agent.pid = Some(pid);
                    if !preserve_codex_session_status {
                        agent.status = PrimaryStatus::Running;
                    }
                    if let Some(real_start) = real_start {
                        if real_start < agent.started_at {
                            agent.started_at = real_start;
                        }
                    }
                    agent.last_activity = now;
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
                            agent.cwd = cwd.clone();
                            agent.pid = Some(pid);
                            agent.status = PrimaryStatus::Running;
                            if let Some(real_start) = real_start {
                                if real_start < agent.started_at {
                                    agent.started_at = real_start;
                                }
                            }
                            agent.last_activity = now;
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
            id,
            pid,
            status: PrimaryStatus::Running,
            normalized,
            activity: None,
            cwd,
            branch: None,
            is_worktree: false,
            started_at: real_start.unwrap_or(now),
            last_activity: now,
            subagents: Vec::new(),
            recent_events: VecDeque::new(),
            session_id,
            alias: None,
            todos: Vec::new(),
        });
        self.mark_agents_dirty();
        if self.root.overview.selected_index.is_none() {
            self.root.overview.selected_index = Some(0);
        }
        self.clamp_selection();
    }

    pub(super) fn handle_status_changed(
        &mut self,
        id: AgentId,
        pid: Option<u32>,
        old: PrimaryStatus,
        new: PrimaryStatus,
        now: chrono::DateTime<chrono::Local>,
    ) {
        if let Some(agent) = self.find_agent_mut_by_id(&id) {
            agent.pid = pid.or(agent.pid);
            agent.status = new;
            agent.last_activity = now;
            push_event(
                &mut agent.recent_events,
                ActivityEventKind::StatusChanged,
                format!("{} → {}", old.as_str(), new.as_str()),
                now,
            );
            self.mark_agents_dirty();
        }
    }

    pub(super) fn handle_activity_changed(
        &mut self,
        id: AgentId,
        pid: Option<u32>,
        activity: Option<Activity>,
        now: chrono::DateTime<chrono::Local>,
    ) {
        if let Some(agent) = self.find_agent_mut_by_id(&id) {
            agent.pid = pid.or(agent.pid);
            agent.activity = activity;
            agent.last_activity = now;
            self.mark_agents_dirty();
        }
    }

    pub(super) fn handle_normalized_state_changed(
        &mut self,
        id: AgentId,
        pid: Option<u32>,
        state: NormalizedState,
        now: chrono::DateTime<chrono::Local>,
    ) {
        if let Some(agent) = self.find_agent_mut_by_id(&id) {
            agent.pid = pid.or(agent.pid);
            agent.normalized = state;
            agent.last_activity = now;
        } else {
            let agent = self.insert_placeholder_agent(id, pid, now);
            agent.normalized = state;
        }
        self.mark_agents_dirty();
        self.clamp_selection();
    }

    pub(super) fn handle_subagent_spawned(
        &mut self,
        id: AgentId,
        pid: Option<u32>,
        subagent: Subagent,
        now: chrono::DateTime<chrono::Local>,
    ) {
        if let Some(agent) = self.find_agent_mut_by_id(&id) {
            agent.pid = pid.or(agent.pid);
            let desc = format!("spawned: {}", subagent.name);
            agent.subagents.push(subagent);
            agent.last_activity = now;
            push_event(
                &mut agent.recent_events,
                ActivityEventKind::SubagentSpawned,
                desc,
                now,
            );
            self.mark_agents_dirty();
        }
    }

    pub(super) fn handle_subagent_completed(
        &mut self,
        id: AgentId,
        pid: Option<u32>,
        subagent_name: String,
        now: chrono::DateTime<chrono::Local>,
    ) {
        if let Some(agent) = self.find_agent_mut_by_id(&id) {
            agent.pid = pid.or(agent.pid);
            let matching_subagent = if id.tool() == AgentTool::Codex {
                agent.subagents.iter_mut().find(|s| {
                    s.name == subagent_name
                        || s.description.as_deref() == Some(subagent_name.as_str())
                })
            } else {
                agent.subagents.iter_mut().find(|s| s.name == subagent_name)
            };
            if let Some(sub) = matching_subagent {
                sub.status = PrimaryStatus::Done;
                sub.ended_at = Some(now);
            }
            agent.last_activity = now;
            push_event(
                &mut agent.recent_events,
                ActivityEventKind::SubagentCompleted,
                format!("completed: {}", subagent_name),
                now,
            );
            self.mark_agents_dirty();
        }
    }

    pub(super) fn handle_subagents_updated(
        &mut self,
        id: AgentId,
        pid: Option<u32>,
        subagents: Vec<Subagent>,
        now: chrono::DateTime<chrono::Local>,
    ) {
        if let Some(agent) = self.find_agent_mut_by_id(&id) {
            agent.pid = pid.or(agent.pid);
            agent.subagents = subagents;
            if agent.subagents.is_empty() {
                agent.recent_events.retain(|event| {
                    !matches!(
                        event.kind,
                        ActivityEventKind::SubagentSpawned | ActivityEventKind::SubagentCompleted
                    )
                });
            }
            agent.last_activity = now;
            self.mark_agents_dirty();
        }
    }

    pub(super) fn handle_tool_call_started(
        &mut self,
        id: AgentId,
        pid: Option<u32>,
        tool_name: String,
        args_summary: Option<String>,
        now: chrono::DateTime<chrono::Local>,
    ) {
        if let Some(agent) = self.find_agent_mut_by_id(&id) {
            agent.pid = pid.or(agent.pid);
            agent.activity = Some(Activity::ToolCall);
            agent.last_activity = now;
            let desc = match args_summary {
                Some(args) => format!("{} ({})", tool_name, args),
                None => tool_name,
            };
            push_event(
                &mut agent.recent_events,
                ActivityEventKind::ToolCall,
                desc,
                now,
            );
            self.mark_agents_dirty();
        }
    }

    pub(super) fn handle_tool_call_completed(
        &mut self,
        id: AgentId,
        pid: Option<u32>,
        now: chrono::DateTime<chrono::Local>,
    ) {
        if let Some(agent) = self.find_agent_mut_by_id(&id) {
            agent.pid = pid.or(agent.pid);
            if agent.activity == Some(Activity::ToolCall) {
                agent.activity = Some(Activity::Idle);
            }
            agent.last_activity = now;
            self.mark_agents_dirty();
        }
    }

    pub(super) fn handle_process_exited(
        &mut self,
        id: AgentId,
        pid: Option<u32>,
        now: chrono::DateTime<chrono::Local>,
    ) {
        // Claude and OpenCode agents are always process-type; remove them immediately
        // so they leave the dashboard when the session closes.
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

    pub(super) fn handle_session_info(
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
                    agent.pid
                };
                self.mark_agents_dirty();
                self.remove_redundant_codex_process_agents(absorbed_pid);
                self.clamp_selection();
                return;
            }

            if let Some(pid) = pid {
                if let Some(agent) = self.find_process_agent_mut(id.tool(), pid) {
                    agent.id = id.clone();
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

    pub(super) fn handle_todos_updated(
        &mut self,
        id: AgentId,
        pid: Option<u32>,
        todos: Vec<TodoItem>,
        now: chrono::DateTime<chrono::Local>,
    ) {
        if let Some(agent) = self.find_agent_mut_by_id(&id) {
            agent.pid = pid.or(agent.pid);
            agent.todos = todos;
            agent.last_activity = now;
            self.mark_agents_dirty();
        }
    }

    pub(super) fn mark_agent_exited(
        agent: &mut Agent,
        pid: Option<u32>,
        now: chrono::DateTime<chrono::Local>,
    ) {
        agent.pid = pid.or(agent.pid);
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

    pub(super) fn apply_session_info(
        agent: &mut Agent,
        pid: Option<u32>,
        session_id: String,
        branch: Option<String>,
        is_worktree: bool,
        started_at: Option<chrono::DateTime<chrono::Local>>,
        now: chrono::DateTime<chrono::Local>,
    ) {
        agent.pid = pid.or(agent.pid);
        if !session_id.is_empty() {
            agent.session_id = Some(session_id);
        }
        if branch.is_some() {
            agent.branch = branch;
        }
        agent.is_worktree = is_worktree;
        if let Some(real_start) = started_at {
            if real_start < agent.started_at {
                agent.started_at = real_start;
            }
        }
        agent.last_activity = now;
    }
}
