use std::collections::VecDeque;
use std::path::PathBuf;

use chrono::Local;

use crate::config::Config;
use crate::event::AgentEvent;
use crate::types::*;

const MAX_RECENT_EVENTS: usize = 20;

pub struct AppState {
    pub agents: Vec<Agent>,
    pub selected: Option<usize>,
    pub sort_mode: SortMode,
    pub show_help: bool,
    pub filter: Option<String>,
    pub filter_mode: bool,
    pub filter_input: String,
    pub rename_mode: bool,
    pub rename_input: String,
    pub config: Config,
    pub should_quit: bool,
}

impl AppState {
    pub fn new(config: Config) -> Self {
        Self {
            agents: Vec::new(),
            selected: None,
            sort_mode: SortMode::Name,
            show_help: false,
            filter: None,
            filter_mode: false,
            filter_input: String::new(),
            rename_mode: false,
            rename_input: String::new(),
            config,
            should_quit: false,
        }
    }

    pub fn handle_event(&mut self, event: AgentEvent) {
        match event {
            AgentEvent::Discovered {
                id,
                pid,
                cwd,
                started_at: real_start,
            } => {
                if self.agents.iter().any(|a| a.id == id) {
                    return;
                }
                let now = Local::now();
                let pid = pid.or_else(|| id.pid());
                let session_id = id.session_id().map(str::to_string);
                if id.tool() == AgentTool::Codex {
                    if let Some(pid) = pid {
                        let matching_session_agents = self
                            .agents
                            .iter()
                            .filter(|a| {
                                a.id.tool() == id.tool()
                                    && a.id.is_session()
                                    && a.pid == Some(pid)
                                    && (a.cwd.as_os_str().is_empty() || a.cwd == cwd)
                            })
                            .count();
                        if matching_session_agents > 1 {
                            return;
                        }
                        if let Some(agent_idx) =
                            self.find_unique_session_agent_index_by_pid(id.tool(), pid, &cwd)
                        {
                            let agent = &mut self.agents[agent_idx];
                            agent.cwd = cwd;
                            agent.pid = Some(pid);
                            if let Some(real_start) = real_start {
                                if real_start < agent.started_at {
                                    agent.started_at = real_start;
                                }
                            }
                            agent.last_activity = now;
                            return;
                        }
                    }
                }
                self.agents.push(Agent {
                    id,
                    pid,
                    status: PrimaryStatus::Running,
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
                if self.selected.is_none() {
                    self.selected = Some(0);
                }
            }

            AgentEvent::StatusChanged { id, pid, old, new } => {
                if let Some(agent) = self.find_agent_mut_by_id(&id) {
                    agent.pid = pid.or(agent.pid);
                    agent.status = new;
                    agent.last_activity = Local::now();
                    push_event(
                        &mut agent.recent_events,
                        ActivityEventKind::StatusChanged,
                        format!("{} → {}", old.as_str(), new.as_str()),
                    );
                }
            }

            AgentEvent::ActivityChanged { id, pid, activity } => {
                if let Some(agent) = self.find_agent_mut_by_id(&id) {
                    agent.pid = pid.or(agent.pid);
                    agent.activity = activity;
                    agent.last_activity = Local::now();
                }
            }

            AgentEvent::SubagentSpawned { id, pid, subagent } => {
                if let Some(agent) = self.find_agent_mut_by_id(&id) {
                    agent.pid = pid.or(agent.pid);
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
                id,
                pid,
                subagent_name,
            } => {
                if let Some(agent) = self.find_agent_mut_by_id(&id) {
                    agent.pid = pid.or(agent.pid);
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
                id,
                pid,
                tool_name,
                args_summary,
            } => {
                if let Some(agent) = self.find_agent_mut_by_id(&id) {
                    agent.pid = pid.or(agent.pid);
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
                id,
                pid,
                tool_name: _,
            } => {
                if let Some(agent) = self.find_agent_mut_by_id(&id) {
                    agent.pid = pid.or(agent.pid);
                    if agent.activity == Some(Activity::ToolCall) {
                        agent.activity = Some(Activity::Idle);
                    }
                    agent.last_activity = Local::now();
                }
            }

            AgentEvent::ProcessExited { id, pid } => {
                if let Some(agent) = self.find_agent_mut_by_id(&id) {
                    agent.pid = pid.or(agent.pid);
                    agent.status = PrimaryStatus::Exited;
                    agent.activity = None;
                    agent.last_activity = Local::now();
                    push_event(
                        &mut agent.recent_events,
                        ActivityEventKind::StatusChanged,
                        "Process exited".to_string(),
                    );
                } else if let Some(pid) = pid {
                    if let Some(agent) = self.find_agent_mut_by_pid(id.tool(), pid) {
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
            }

            AgentEvent::SessionInfo {
                id,
                pid,
                session_id,
                branch,
                is_worktree,
                started_at,
            } => {
                let pid = pid.or_else(|| id.pid());
                if id.is_session() {
                    if let Some(agent) = self.find_agent_mut_by_id(&id) {
                        Self::apply_session_info(agent, pid, session_id, branch, is_worktree, started_at);
                        return;
                    }

                    if let Some(pid) = pid {
                        if let Some(agent) = self.find_process_agent_mut(id.tool(), pid) {
                            agent.id = id.clone();
                            Self::apply_session_info(
                                agent,
                                Some(pid),
                                session_id,
                                branch,
                                is_worktree,
                                started_at,
                            );
                            return;
                        }
                    }

                    let now = Local::now();
                    self.agents.push(Agent {
                        id,
                        pid,
                        status: PrimaryStatus::Running,
                        activity: None,
                        cwd: PathBuf::new(),
                        branch,
                        is_worktree,
                        started_at: started_at.unwrap_or(now),
                        last_activity: now,
                        subagents: Vec::new(),
                        recent_events: VecDeque::new(),
                        session_id: Some(session_id),
                        alias: None,
                        todos: Vec::new(),
                    });
                    if self.selected.is_none() {
                        self.selected = Some(0);
                    }
                    return;
                }

                if let Some(agent) = self.find_agent_mut_by_id(&id) {
                    Self::apply_session_info(agent, pid, session_id, branch, is_worktree, started_at);
                }
            }

            AgentEvent::TodosUpdated { id, pid, todos } => {
                if let Some(agent) = self.find_agent_mut_by_id(&id) {
                    agent.pid = pid.or(agent.pid);
                    agent.todos = todos;
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
                a.display_name()
                    .cmp(b.display_name())
                    .then_with(|| a.pid.cmp(&b.pid))
                    .then_with(|| a.id.session_id().cmp(&b.id.session_id()))
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

    pub fn toggle_help(&mut self) {
        self.show_help = !self.show_help;
    }

    pub fn dismiss_overlay(&mut self) {
        self.show_help = false;
        self.filter_mode = false;
        self.filter = None;
        self.filter_input.clear();
        self.rename_mode = false;
        self.rename_input.clear();
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

    /// Resolve the currently selected visible index to the corresponding
    /// `AgentId` and its position in `self.agents`.
    fn resolve_selected(&self) -> Option<(usize, AgentId)> {
        let vis_idx = self.selected?;
        let visible = self.visible_agents();
        let agent = visible.get(vis_idx)?;
        let id = agent.id.clone();
        let real_idx = self.agents.iter().position(|a| a.id == id)?;
        Some((real_idx, id))
    }

    pub fn start_rename(&mut self) {
        let (real_idx, _id) = match self.resolve_selected() {
            Some(v) => v,
            None => return,
        };
        self.rename_mode = true;
        self.rename_input = self.agents[real_idx].display_name().to_string();
    }

    pub fn rename_push_char(&mut self, c: char) {
        self.rename_input.push(c);
    }

    pub fn rename_pop_char(&mut self) {
        self.rename_input.pop();
    }

    pub fn confirm_rename(&mut self) {
        if let Some((real_idx, _id)) = self.resolve_selected() {
            if real_idx < self.agents.len() {
                if self.rename_input.is_empty() {
                    self.agents[real_idx].alias = None;
                } else {
                    self.agents[real_idx].alias = Some(self.rename_input.clone());
                }
            }
        }
        self.rename_mode = false;
        self.rename_input.clear();
    }

    pub fn cancel_rename(&mut self) {
        self.rename_mode = false;
        self.rename_input.clear();
    }

    pub fn remove_exited(&mut self) {
        self.agents.retain(|a| a.status != PrimaryStatus::Exited);
        self.clamp_selection();
    }

    pub fn remove_selected(&mut self) {
        let (real_idx, _id) = match self.resolve_selected() {
            Some(v) => v,
            None => return,
        };
        self.agents.remove(real_idx);
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

    fn find_agent_mut_by_id(&mut self, id: &AgentId) -> Option<&mut Agent> {
        self.agents.iter_mut().find(|a| a.id == *id)
    }

    fn find_process_agent_mut(&mut self, tool: AgentTool, pid: u32) -> Option<&mut Agent> {
        self.agents
            .iter_mut()
            .find(|a| a.id.tool() == tool && a.pid == Some(pid) && a.id.is_process())
    }

    fn find_unique_session_agent_index_by_pid(
        &self,
        tool: AgentTool,
        pid: u32,
        cwd: &PathBuf,
    ) -> Option<usize> {
        let mut matches = self
            .agents
            .iter()
            .enumerate()
            .filter(|(_, a)| {
                a.id.tool() == tool
                    && a.id.is_session()
                    && a.pid == Some(pid)
                    && (a.cwd.as_os_str().is_empty() || a.cwd == *cwd)
            });
        let (idx, _) = matches.next()?;
        if matches.next().is_none() {
            Some(idx)
        } else {
            None
        }
    }

    fn find_agent_mut_by_pid(&mut self, tool: AgentTool, pid: u32) -> Option<&mut Agent> {
        self.agents
            .iter_mut()
            .find(|a| a.id.tool() == tool && a.pid == Some(pid))
    }

    fn apply_session_info(
        agent: &mut Agent,
        pid: Option<u32>,
        session_id: String,
        branch: Option<String>,
        is_worktree: bool,
        started_at: Option<chrono::DateTime<chrono::Local>>,
    ) {
        agent.pid = pid.or(agent.pid);
        agent.session_id = Some(session_id);
        if branch.is_some() {
            agent.branch = branch;
        }
        agent.is_worktree = is_worktree;
        if let Some(real_start) = started_at {
            if real_start < agent.started_at {
                agent.started_at = real_start;
            }
        }
        agent.last_activity = Local::now();
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
