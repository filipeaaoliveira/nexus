use std::cell::{Cell, RefCell};
use std::cmp::Reverse;
use std::collections::VecDeque;
use std::path::PathBuf;

use chrono::Local;

use crate::config::Config;
use crate::event::AgentEvent;
use crate::types::*;

const MAX_RECENT_EVENTS: usize = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveTab {
    Overview,
    Codex,
}

impl ActiveTab {
    pub const ALL: [Self; 2] = [Self::Overview, Self::Codex];

    pub fn next(self) -> Self {
        match self {
            Self::Overview => Self::Codex,
            Self::Codex => Self::Overview,
        }
    }

    pub fn previous(self) -> Self {
        match self {
            Self::Overview => Self::Codex,
            Self::Codex => Self::Overview,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Overview => "Overview",
            Self::Codex => "Codex",
        }
    }
}

#[derive(Debug, Default)]
struct VisibleAgentCache {
    version: u64,
    filter: Option<String>,
    sort_mode: Option<SortMode>,
    indices: Vec<usize>,
}

#[derive(Debug, Clone)]
struct SessionInfoUpdate {
    pid: Option<u32>,
    session_id: String,
    branch: Option<String>,
    is_worktree: bool,
    started_at: Option<chrono::DateTime<chrono::Local>>,
}

pub struct AppState {
    active_tab: ActiveTab,
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
    visible_agent_cache: RefCell<VisibleAgentCache>,
    agents_version: Cell<u64>,
}

impl AppState {
    pub fn new(config: Config) -> Self {
        Self {
            active_tab: ActiveTab::Overview,
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
            visible_agent_cache: RefCell::new(VisibleAgentCache::default()),
            agents_version: Cell::new(0),
        }
    }

    pub fn active_tab(&self) -> ActiveTab {
        self.active_tab
    }

    pub fn next_tab(&mut self) {
        self.active_tab = self.active_tab.next();
    }

    pub fn previous_tab(&mut self) {
        self.active_tab = self.active_tab.previous();
    }

    pub fn handle_event(&mut self, event: AgentEvent) {
        let now = Local::now();
        match event {
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
                self.handle_normalized_state_changed(id, pid, state, now)
            }
            AgentEvent::SubagentSpawned { id, pid, subagent } => {
                self.handle_subagent_spawned(id, pid, subagent, now)
            }
            AgentEvent::SubagentCompleted {
                id,
                pid,
                subagent_name,
            } => self.handle_subagent_completed(id, pid, subagent_name, now),
            AgentEvent::ToolCallStarted {
                id,
                pid,
                tool_name,
                args_summary,
            } => self.handle_tool_call_started(id, pid, tool_name, args_summary, now),
            AgentEvent::ToolCallCompleted { id, pid, tool_name } => {
                self.handle_tool_call_completed(id, pid, tool_name, now)
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
    }

    pub fn visible_agents(&self) -> Vec<&Agent> {
        self.with_visible_agent_indices(|indices| {
            indices.iter().map(|&idx| &self.agents[idx]).collect()
        })
    }

    pub fn count_by_status(&self, status: PrimaryStatus) -> usize {
        self.with_visible_agent_indices(|indices| {
            indices
                .iter()
                .filter(|&&idx| self.agents[idx].status == status)
                .count()
        })
    }

    pub fn move_selection_down(&mut self) {
        let count = self.visible_agent_count();
        if count == 0 {
            return;
        }
        let max = count - 1;
        self.selected = Some(self.selected.map_or(0, |s| (s + 1).min(max)));
    }

    pub fn move_selection_up(&mut self) {
        let count = self.visible_agent_count();
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
        self.with_visible_agent_indices(|indices| {
            let real_idx = *indices.get(vis_idx)?;
            let id = self.agents[real_idx].id.clone();
            Some((real_idx, id))
        })
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
                self.mark_agents_dirty();
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
        self.mark_agents_dirty();
        self.clamp_selection();
    }

    pub fn remove_selected(&mut self) {
        let (real_idx, _id) = match self.resolve_selected() {
            Some(v) => v,
            None => return,
        };
        self.agents.remove(real_idx);
        self.mark_agents_dirty();
        self.clamp_selection();
    }

    fn clamp_selection(&mut self) {
        let count = self.visible_agent_count();
        if count == 0 {
            self.selected = None;
        } else if self.selected.is_none() {
            self.selected = Some(0);
        } else if let Some(idx) = self.selected {
            if idx >= count {
                self.selected = Some(count - 1);
            }
        }
    }

    fn find_agent_mut_by_id(&mut self, id: &AgentId) -> Option<&mut Agent> {
        self.agents.iter_mut().find(|a| a.id == *id)
    }

    fn insert_placeholder_agent(
        &mut self,
        id: AgentId,
        pid: Option<u32>,
        now: chrono::DateTime<chrono::Local>,
    ) -> &mut Agent {
        if let Some(agent_idx) = self.agents.iter().position(|agent| agent.id == id) {
            return &mut self.agents[agent_idx];
        }

        let session_id = id.session_id().map(str::to_string);
        self.agents.push(Agent {
            id,
            pid,
            status: PrimaryStatus::Running,
            normalized: NormalizedState::new(NormalizedStatus::Working, None),
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

    fn find_process_agent_mut(&mut self, tool: AgentTool, pid: u32) -> Option<&mut Agent> {
        self.agents
            .iter_mut()
            .find(|a| a.id.tool() == tool && a.pid == Some(pid) && a.id.is_process())
    }

    fn find_session_agent_indices_by_pid(
        &self,
        tool: AgentTool,
        pid: u32,
        cwd: &PathBuf,
    ) -> Vec<usize> {
        self.agents
            .iter()
            .enumerate()
            .filter(|(_, a)| {
                a.id.tool() == tool
                    && a.id.is_session()
                    && a.pid == Some(pid)
                    && a.status != PrimaryStatus::Exited
                    && (a.cwd.as_os_str().is_empty() || a.cwd == *cwd)
            })
            .map(|(idx, _)| idx)
            .collect()
    }

    fn find_agent_mut_by_pid(&mut self, tool: AgentTool, pid: u32) -> Option<&mut Agent> {
        self.agents
            .iter_mut()
            .find(|a| a.id.tool() == tool && a.pid == Some(pid))
    }

    fn remove_redundant_codex_process_agents(&mut self, pid: Option<u32>) {
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

    fn handle_discovered(
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
            }
        }

        self.agents.push(Agent {
            id,
            pid,
            status: PrimaryStatus::Running,
            normalized: NormalizedState::new(NormalizedStatus::Working, None),
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
        if self.selected.is_none() {
            self.selected = Some(0);
        }
        self.clamp_selection();
    }

    fn handle_status_changed(
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

    fn handle_activity_changed(
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

    fn handle_normalized_state_changed(
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

    fn handle_subagent_spawned(
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

    fn handle_subagent_completed(
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

    fn handle_tool_call_started(
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

    fn handle_tool_call_completed(
        &mut self,
        id: AgentId,
        pid: Option<u32>,
        _tool_name: String,
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

    fn handle_process_exited(
        &mut self,
        id: AgentId,
        pid: Option<u32>,
        now: chrono::DateTime<chrono::Local>,
    ) {
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

    fn handle_session_info(
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

    fn handle_todos_updated(
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

    fn mark_agent_exited(
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

    fn visible_agent_count(&self) -> usize {
        self.with_visible_agent_indices(|indices| indices.len())
    }

    fn with_visible_agent_indices<R>(&self, f: impl FnOnce(&[usize]) -> R) -> R {
        self.refresh_visible_agent_cache();
        let cache = self.visible_agent_cache.borrow();
        f(&cache.indices)
    }

    fn refresh_visible_agent_cache(&self) {
        let current_version = self.agents_version.get();
        let current_filter = self.filter.clone();
        let current_sort_mode = self.sort_mode;
        let should_refresh = {
            let cache = self.visible_agent_cache.borrow();
            cache.version != current_version
                || cache.filter != current_filter
                || cache.sort_mode != Some(current_sort_mode)
        };

        if !should_refresh {
            return;
        }

        let indices = Self::compute_visible_agent_indices(
            &self.agents,
            current_filter.as_deref(),
            current_sort_mode,
        );
        *self.visible_agent_cache.borrow_mut() = VisibleAgentCache {
            version: current_version,
            filter: current_filter,
            sort_mode: Some(current_sort_mode),
            indices,
        };
    }

    fn compute_visible_agent_indices(
        agents: &[Agent],
        filter: Option<&str>,
        sort_mode: SortMode,
    ) -> Vec<usize> {
        let mut indices: Vec<usize> = agents
            .iter()
            .enumerate()
            .filter(|(_, agent)| {
                !(agent.id.tool() == AgentTool::Codex
                    && agent.id.is_session()
                    && agent.pid.is_none())
            })
            .map(|(idx, _)| idx)
            .collect();

        if let Some(filter) = filter {
            let filter = filter.to_lowercase();
            indices.retain(|&idx| Self::agent_matches_filter(&agents[idx], &filter));
        }

        match sort_mode {
            SortMode::Urgency => {
                indices.sort_by_key(|&idx| {
                    let agent = &agents[idx];
                    (Reverse(agent.normalized.status), agent.status)
                });
            }
            SortMode::Name => {
                indices.sort_by(|&left, &right| {
                    let a = &agents[left];
                    let b = &agents[right];
                    a.display_name()
                        .cmp(b.display_name())
                        .then_with(|| a.pid.cmp(&b.pid))
                        .then_with(|| a.id.session_id().cmp(&b.id.session_id()))
                });
            }
            SortMode::Elapsed => {
                indices.sort_by(|&left, &right| {
                    agents[left].started_at.cmp(&agents[right].started_at)
                });
            }
        }

        indices
    }

    fn agent_matches_filter(agent: &Agent, filter: &str) -> bool {
        agent.display_name().to_lowercase().contains(filter)
            || agent.status.as_str().to_lowercase().contains(filter)
            || agent
                .normalized
                .status
                .as_str()
                .to_lowercase()
                .contains(filter)
            || agent
                .normalized
                .reason
                .as_ref()
                .map(|reason| reason.as_str().to_lowercase().contains(filter))
                .unwrap_or(false)
            || agent.cwd.to_string_lossy().to_lowercase().contains(filter)
            || agent
                .branch
                .as_deref()
                .map(|branch| branch.to_lowercase().contains(filter))
                .unwrap_or(false)
    }

    fn mark_agents_dirty(&self) {
        self.agents_version.set(self.agents_version.get() + 1);
    }

    fn apply_session_info(
        agent: &mut Agent,
        pid: Option<u32>,
        session_id: String,
        branch: Option<String>,
        is_worktree: bool,
        started_at: Option<chrono::DateTime<chrono::Local>>,
        now: chrono::DateTime<chrono::Local>,
    ) {
        if agent.id.tool() == AgentTool::Codex && agent.id.is_session() {
            agent.pid = pid;
        } else {
            agent.pid = pid.or(agent.pid);
        }
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
        agent.last_activity = now;
    }
}

fn push_event(
    events: &mut VecDeque<ActivityEvent>,
    kind: ActivityEventKind,
    description: String,
    now: chrono::DateTime<Local>,
) {
    events.push_back(ActivityEvent {
        timestamp: now,
        kind,
        description,
    });
    while events.len() > MAX_RECENT_EVENTS {
        events.pop_front();
    }
}

#[cfg(test)]
mod tests {
    use super::AppState;
    use crate::config::Config;
    use crate::event::AgentEvent;
    use crate::types::{AgentId, AgentTool, PrimaryStatus, SortMode};
    use std::path::PathBuf;

    fn discovered(tool: AgentTool, pid: u32, cwd: &str) -> AgentEvent {
        AgentEvent::Discovered {
            id: AgentId::new(tool, pid),
            pid: Some(pid),
            cwd: PathBuf::from(cwd),
            started_at: None,
        }
    }

    #[test]
    fn visible_agent_indices_refresh_when_filter_changes() {
        let mut state = AppState::new(Config::default());
        state.handle_event(discovered(AgentTool::Claude, 1, "/tmp/claude"));
        state.handle_event(discovered(AgentTool::OpenCode, 2, "/tmp/opencode"));

        assert_eq!(state.visible_agent_count(), 2);

        state.filter = Some("claude".to_string());
        assert_eq!(state.visible_agent_count(), 1);
        assert_eq!(state.visible_agents()[0].pid, Some(1));

        state.filter = Some("opencode".to_string());
        assert_eq!(state.visible_agent_count(), 1);
        assert_eq!(state.visible_agents()[0].pid, Some(2));
    }

    #[test]
    fn visible_agent_indices_refresh_when_sort_changes() {
        let mut state = AppState::new(Config::default());
        state.handle_event(discovered(AgentTool::OpenCode, 2, "/tmp/opencode"));
        state.handle_event(discovered(AgentTool::Claude, 1, "/tmp/claude"));

        assert_eq!(state.visible_agents()[0].pid, Some(1));

        state.sort_mode = SortMode::Elapsed;
        assert_eq!(state.visible_agents()[0].pid, Some(2));

        state.handle_event(AgentEvent::StatusChanged {
            id: AgentId::new(AgentTool::OpenCode, 2),
            pid: Some(2),
            old: PrimaryStatus::Running,
            new: PrimaryStatus::Waiting,
        });
        state.sort_mode = SortMode::Urgency;
        assert_eq!(state.visible_agents()[0].pid, Some(2));
    }
}
