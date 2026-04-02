use std::collections::VecDeque;

use chrono::Local;

use crate::config::Config;
use crate::event::AgentEvent;
use crate::types::*;

const MAX_RECENT_EVENTS: usize = 20;

pub struct AppState {
    pub agents: Vec<Agent>,
    pub selected: Option<usize>,
    pub sort_mode: SortMode,
    pub group_by_worktree: bool,
    pub show_help: bool,
    pub filter: Option<String>,
    pub filter_mode: bool,
    pub filter_input: String,
    pub config: Config,
    pub should_quit: bool,
}

impl AppState {
    pub fn new(config: Config) -> Self {
        Self {
            agents: Vec::new(),
            selected: None,
            sort_mode: SortMode::Urgency,
            group_by_worktree: false,
            show_help: false,
            filter: None,
            filter_mode: false,
            filter_input: String::new(),
            config,
            should_quit: false,
        }
    }

    pub fn handle_event(&mut self, event: AgentEvent) {
        match event {
            AgentEvent::Discovered {
                tool,
                pid,
                cwd,
                started_at: real_start,
            } => {
                let id = AgentId::new(tool, pid);
                if self.agents.iter().any(|a| a.id == id) {
                    return;
                }
                let now = Local::now();
                self.agents.push(Agent {
                    id,
                    status: PrimaryStatus::Running,
                    activity: None,
                    cwd,
                    branch: None,
                    is_worktree: false,
                    started_at: real_start.unwrap_or(now),
                    last_activity: now,
                    subagents: Vec::new(),
                    recent_events: VecDeque::new(),
                    session_id: None,
                });
                if self.selected.is_none() {
                    self.selected = Some(0);
                }
            }

            AgentEvent::StatusChanged {
                pid,
                tool,
                old,
                new,
            } => {
                if let Some(agent) = self.find_agent_mut(pid, tool) {
                    agent.status = new;
                    agent.last_activity = Local::now();
                    push_event(
                        &mut agent.recent_events,
                        ActivityEventKind::StatusChanged,
                        format!("{} → {}", old.as_str(), new.as_str()),
                    );
                }
            }

            AgentEvent::ActivityChanged {
                pid,
                tool,
                activity,
            } => {
                if let Some(agent) = self.find_agent_mut(pid, tool) {
                    agent.activity = activity;
                    agent.last_activity = Local::now();
                }
            }

            AgentEvent::SubagentSpawned {
                pid,
                tool,
                subagent,
            } => {
                if let Some(agent) = self.find_agent_mut(pid, tool) {
                    let desc = format!("spawned: {}", subagent.name);
                    agent.subagents.push(subagent);
                    agent.last_activity = Local::now();
                    push_event(
                        &mut agent.recent_events,
                        ActivityEventKind::SubagentSpawned,
                        desc,
                    );
                }
            }

            AgentEvent::SubagentCompleted {
                pid,
                tool,
                subagent_name,
            } => {
                if let Some(agent) = self.find_agent_mut(pid, tool) {
                    if let Some(sub) = agent.subagents.iter_mut().find(|s| s.name == subagent_name)
                    {
                        sub.status = PrimaryStatus::Done;
                        sub.ended_at = Some(Local::now());
                    }
                    agent.last_activity = Local::now();
                    push_event(
                        &mut agent.recent_events,
                        ActivityEventKind::SubagentCompleted,
                        format!("completed: {}", subagent_name),
                    );
                }
            }

            AgentEvent::ToolCallStarted {
                pid,
                tool,
                tool_name,
                args_summary,
            } => {
                if let Some(agent) = self.find_agent_mut(pid, tool) {
                    agent.activity = Some(Activity::ToolCall);
                    agent.last_activity = Local::now();
                    let desc = match args_summary {
                        Some(args) => format!("{} ({})", tool_name, args),
                        None => tool_name,
                    };
                    push_event(&mut agent.recent_events, ActivityEventKind::ToolCall, desc);
                }
            }

            AgentEvent::ToolCallCompleted {
                pid,
                tool,
                tool_name: _,
            } => {
                if let Some(agent) = self.find_agent_mut(pid, tool) {
                    if agent.activity == Some(Activity::ToolCall) {
                        agent.activity = Some(Activity::Idle);
                    }
                    agent.last_activity = Local::now();
                }
            }

            AgentEvent::ProcessExited { pid, tool } => {
                if let Some(agent) = self.find_agent_mut(pid, tool) {
                    agent.status = PrimaryStatus::Exited;
                    agent.activity = None;
                    agent.last_activity = Local::now();
                    push_event(
                        &mut agent.recent_events,
                        ActivityEventKind::StatusChanged,
                        "Process exited".to_string(),
                    );
                }
            }

            AgentEvent::SessionInfo {
                pid,
                tool,
                session_id,
                branch,
                is_worktree,
                started_at,
            } => {
                if let Some(agent) = self.find_agent_mut(pid, tool) {
                    agent.session_id = Some(session_id);
                    if branch.is_some() {
                        agent.branch = branch;
                    }
                    agent.is_worktree = is_worktree;
                    // Apply real start time if available and earlier than current
                    if let Some(real_start) = started_at {
                        if real_start < agent.started_at {
                            agent.started_at = real_start;
                        }
                    }
                    agent.last_activity = Local::now();
                }
            }
        }
    }

    pub fn visible_agents(&self) -> Vec<&Agent> {
        let mut agents: Vec<&Agent> = self.agents.iter().collect();

        // Apply filter
        if let Some(ref filter) = self.filter {
            let f = filter.to_lowercase();
            agents.retain(|a| {
                a.display_name().to_lowercase().contains(&f)
                    || a.status.as_str().to_lowercase().contains(&f)
                    || a.cwd.to_string_lossy().to_lowercase().contains(&f)
                    || a.branch
                        .as_deref()
                        .map(|b| b.to_lowercase().contains(&f))
                        .unwrap_or(false)
            });
        }

        // Apply sort
        match self.sort_mode {
            SortMode::Urgency => agents.sort_by_key(|a| a.status),
            SortMode::Name => agents.sort_by(|a, b| {
                a.id.tool
                    .as_str()
                    .cmp(b.id.tool.as_str())
                    .then_with(|| a.id.pid.cmp(&b.id.pid))
            }),
            SortMode::Elapsed => {
                agents.sort_by(|a, b| a.started_at.cmp(&b.started_at));
            }
        }

        agents
    }

    pub fn count_by_status(&self, status: PrimaryStatus) -> usize {
        self.agents.iter().filter(|a| a.status == status).count()
    }

    pub fn move_selection_down(&mut self) {
        let count = self.visible_agents().len();
        if count == 0 {
            return;
        }
        let max = count - 1;
        self.selected = Some(self.selected.map_or(0, |s| (s + 1).min(max)));
    }

    pub fn move_selection_up(&mut self) {
        let count = self.visible_agents().len();
        if count == 0 {
            return;
        }
        self.selected = Some(self.selected.map_or(0, |s| s.saturating_sub(1)));
    }

    pub fn cycle_sort_mode(&mut self) {
        self.sort_mode = self.sort_mode.next();
    }

    pub fn toggle_group_by_worktree(&mut self) {
        self.group_by_worktree = !self.group_by_worktree;
    }

    pub fn toggle_help(&mut self) {
        self.show_help = !self.show_help;
    }

    pub fn dismiss_overlay(&mut self) {
        self.show_help = false;
        self.filter_mode = false;
        self.filter = None;
        self.filter_input.clear();
        self.clamp_selection();
    }

    pub fn start_filter(&mut self) {
        self.filter_mode = true;
        self.filter_input.clear();
        self.filter = None;
    }

    pub fn filter_push_char(&mut self, c: char) {
        self.filter_input.push(c);
        self.sync_filter();
    }

    pub fn filter_pop_char(&mut self) {
        self.filter_input.pop();
        self.sync_filter();
    }

    fn sync_filter(&mut self) {
        self.filter = if self.filter_input.is_empty() {
            None
        } else {
            Some(self.filter_input.clone())
        };
        self.clamp_selection();
    }

    pub fn confirm_filter(&mut self) {
        self.filter_mode = false;
        // Keep filter active
        self.clamp_selection();
    }

    fn clamp_selection(&mut self) {
        let count = self.visible_agents().len();
        if count == 0 {
            self.selected = None;
        } else if let Some(idx) = self.selected {
            if idx >= count {
                self.selected = Some(count - 1);
            }
        }
    }

    fn find_agent_mut(&mut self, pid: u32, tool: AgentTool) -> Option<&mut Agent> {
        let id = AgentId::new(tool, pid);
        self.agents.iter_mut().find(|a| a.id == id)
    }
}

fn push_event(events: &mut VecDeque<ActivityEvent>, kind: ActivityEventKind, description: String) {
    events.push_back(ActivityEvent {
        timestamp: Local::now(),
        kind,
        description,
    });
    while events.len() > MAX_RECENT_EVENTS {
        events.pop_front();
    }
}
