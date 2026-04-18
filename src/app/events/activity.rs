use super::*;

impl AppState {
    pub(in crate::app) fn handle_status_changed(
        &mut self,
        id: AgentId,
        pid: Option<u32>,
        old: PrimaryStatus,
        new: PrimaryStatus,
        now: chrono::DateTime<chrono::Local>,
    ) {
        if let Some(agent) = self.find_agent_mut_by_id(&id) {
            agent.merge_pid(pid);
            agent.set_status(new);
            agent.touch(now);
            push_event(
                agent.recent_events_mut(),
                ActivityEventKind::StatusChanged,
                format!("{} → {}", old.as_str(), new.as_str()),
                now,
            );
            self.mark_agents_dirty();
        }
    }

    pub(in crate::app) fn handle_activity_changed(
        &mut self,
        id: AgentId,
        pid: Option<u32>,
        activity: Option<Activity>,
        now: chrono::DateTime<chrono::Local>,
    ) {
        if let Some(agent) = self.find_agent_mut_by_id(&id) {
            agent.merge_pid(pid);
            agent.set_activity(activity);
            agent.touch(now);
            self.mark_agents_dirty();
        }
    }

    pub(in crate::app) fn handle_normalized_state_changed(
        &mut self,
        id: AgentId,
        pid: Option<u32>,
        state: NormalizedState,
        now: chrono::DateTime<chrono::Local>,
    ) {
        if let Some(agent) = self.find_agent_mut_by_id(&id) {
            agent.merge_pid(pid);
            agent.set_normalized(state);
            agent.touch(now);
        } else {
            let agent = self.insert_placeholder_agent(id, pid, now);
            agent.set_normalized(state);
        }
        self.mark_agents_dirty();
        self.clamp_selection();
    }

    pub(in crate::app) fn handle_subagent_spawned(
        &mut self,
        id: AgentId,
        pid: Option<u32>,
        subagent: Subagent,
        now: chrono::DateTime<chrono::Local>,
    ) {
        if let Some(agent) = self.find_agent_mut_by_id(&id) {
            agent.merge_pid(pid);
            let desc = format!("spawned: {}", subagent.name);
            agent.push_subagent(subagent);
            agent.touch(now);
            push_event(
                agent.recent_events_mut(),
                ActivityEventKind::SubagentSpawned,
                desc,
                now,
            );
            self.mark_agents_dirty();
        }
    }

    pub(in crate::app) fn handle_subagent_completed(
        &mut self,
        id: AgentId,
        pid: Option<u32>,
        subagent_name: String,
        now: chrono::DateTime<chrono::Local>,
    ) {
        if let Some(agent) = self.find_agent_mut_by_id(&id) {
            agent.merge_pid(pid);
            let matching_subagent = if id.tool() == AgentTool::Codex {
                agent.subagents_mut().iter_mut().find(|s| {
                    s.name == subagent_name
                        || s.description.as_deref() == Some(subagent_name.as_str())
                })
            } else {
                agent
                    .subagents_mut()
                    .iter_mut()
                    .find(|s| s.name == subagent_name)
            };
            if let Some(sub) = matching_subagent {
                sub.status = PrimaryStatus::Done;
                sub.ended_at = Some(now);
            }
            agent.touch(now);
            push_event(
                agent.recent_events_mut(),
                ActivityEventKind::SubagentCompleted,
                format!("completed: {}", subagent_name),
                now,
            );
            self.mark_agents_dirty();
        }
    }

    pub(in crate::app) fn handle_subagents_updated(
        &mut self,
        id: AgentId,
        pid: Option<u32>,
        subagents: Vec<Subagent>,
        now: chrono::DateTime<chrono::Local>,
    ) {
        if let Some(agent) = self.find_agent_mut_by_id(&id) {
            agent.merge_pid(pid);
            agent.set_subagents(subagents);
            if agent.subagents().is_empty() {
                agent.clear_subagent_events();
            }
            agent.touch(now);
            self.mark_agents_dirty();
        }
    }

    pub(in crate::app) fn handle_tool_call_started(
        &mut self,
        id: AgentId,
        pid: Option<u32>,
        tool_name: String,
        args_summary: Option<String>,
        now: chrono::DateTime<chrono::Local>,
    ) {
        if let Some(agent) = self.find_agent_mut_by_id(&id) {
            agent.merge_pid(pid);
            agent.set_activity(Some(Activity::ToolCall));
            agent.touch(now);
            let desc = match args_summary {
                Some(args) => format!("{} ({})", tool_name, args),
                None => tool_name,
            };
            push_event(
                agent.recent_events_mut(),
                ActivityEventKind::ToolCall,
                desc,
                now,
            );
            self.mark_agents_dirty();
        }
    }

    pub(in crate::app) fn handle_tool_call_completed(
        &mut self,
        id: AgentId,
        pid: Option<u32>,
        now: chrono::DateTime<chrono::Local>,
    ) {
        if let Some(agent) = self.find_agent_mut_by_id(&id) {
            agent.merge_pid(pid);
            if agent.activity() == Some(Activity::ToolCall) {
                agent.set_activity(Some(Activity::Idle));
            }
            agent.touch(now);
            self.mark_agents_dirty();
        }
    }

    pub(in crate::app) fn handle_todos_updated(
        &mut self,
        id: AgentId,
        pid: Option<u32>,
        todos: Vec<TodoItem>,
        now: chrono::DateTime<chrono::Local>,
    ) {
        if let Some(agent) = self.find_agent_mut_by_id(&id) {
            agent.merge_pid(pid);
            agent.set_todos(todos);
            agent.touch(now);
            self.mark_agents_dirty();
        }
    }
}
