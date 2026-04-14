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
            agent.ident.pid = pid.or(agent.ident.pid);
            agent.lifecycle.status = new;
            agent.lifecycle.last_activity = now;
            push_event(
                &mut agent.detail.recent_events,
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
            agent.ident.pid = pid.or(agent.ident.pid);
            agent.lifecycle.activity = activity;
            agent.lifecycle.last_activity = now;
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
            agent.ident.pid = pid.or(agent.ident.pid);
            agent.lifecycle.normalized = state;
            agent.lifecycle.last_activity = now;
        } else {
            let agent = self.insert_placeholder_agent(id, pid, now);
            agent.lifecycle.normalized = state;
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
            agent.ident.pid = pid.or(agent.ident.pid);
            let desc = format!("spawned: {}", subagent.name);
            agent.detail.subagents.push(subagent);
            agent.lifecycle.last_activity = now;
            push_event(
                &mut agent.detail.recent_events,
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
            agent.ident.pid = pid.or(agent.ident.pid);
            let matching_subagent = if id.tool() == AgentTool::Codex {
                agent.detail.subagents.iter_mut().find(|s| {
                    s.name == subagent_name
                        || s.description.as_deref() == Some(subagent_name.as_str())
                })
            } else {
                agent
                    .detail
                    .subagents
                    .iter_mut()
                    .find(|s| s.name == subagent_name)
            };
            if let Some(sub) = matching_subagent {
                sub.status = PrimaryStatus::Done;
                sub.ended_at = Some(now);
            }
            agent.lifecycle.last_activity = now;
            push_event(
                &mut agent.detail.recent_events,
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
            agent.ident.pid = pid.or(agent.ident.pid);
            agent.detail.subagents = subagents;
            if agent.detail.subagents.is_empty() {
                agent.detail.recent_events.retain(|event| {
                    !matches!(
                        event.kind,
                        ActivityEventKind::SubagentSpawned | ActivityEventKind::SubagentCompleted
                    )
                });
            }
            agent.lifecycle.last_activity = now;
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
            agent.ident.pid = pid.or(agent.ident.pid);
            agent.lifecycle.activity = Some(Activity::ToolCall);
            agent.lifecycle.last_activity = now;
            let desc = match args_summary {
                Some(args) => format!("{} ({})", tool_name, args),
                None => tool_name,
            };
            push_event(
                &mut agent.detail.recent_events,
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
            agent.ident.pid = pid.or(agent.ident.pid);
            if agent.lifecycle.activity == Some(Activity::ToolCall) {
                agent.lifecycle.activity = Some(Activity::Idle);
            }
            agent.lifecycle.last_activity = now;
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
            agent.ident.pid = pid.or(agent.ident.pid);
            agent.detail.todos = todos;
            agent.lifecycle.last_activity = now;
            self.mark_agents_dirty();
        }
    }
}
