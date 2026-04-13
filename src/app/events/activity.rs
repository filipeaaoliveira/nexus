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

    pub(in crate::app) fn handle_activity_changed(
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

    pub(in crate::app) fn handle_normalized_state_changed(
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

    pub(in crate::app) fn handle_subagent_spawned(
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

    pub(in crate::app) fn handle_subagent_completed(
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

    pub(in crate::app) fn handle_subagents_updated(
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

    pub(in crate::app) fn handle_tool_call_started(
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

    pub(in crate::app) fn handle_tool_call_completed(
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

    pub(in crate::app) fn handle_todos_updated(
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
}
