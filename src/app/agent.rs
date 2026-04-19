use std::collections::VecDeque;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Local};

use nexus_domain::agents::activity::{Activity, Subagent, TodoItem};
use nexus_domain::agents::ids::AgentId;
use nexus_domain::agents::status::{NormalizedState, PrimaryStatus};

// ── Sub-structs ──────────────────────────────────────────────

/// Stable identity fields that remain constant across the agent's lifetime.
#[derive(Debug, Clone)]
pub struct AgentIdent {
    pub(crate) id: AgentId,
    pub(crate) pid: Option<u32>,
    pub(crate) session_id: Option<String>,
    pub(crate) alias: Option<String>,
    pub(crate) cwd: Option<PathBuf>,
    pub(crate) branch: Option<String>,
    pub(crate) is_worktree: bool,
    pub(crate) started_at: DateTime<Local>,
}

/// Mutable lifecycle state that changes as the agent progresses.
#[derive(Debug, Clone)]
pub struct AgentLifecycle {
    pub(crate) status: PrimaryStatus,
    pub(crate) normalized: NormalizedState,
    pub(crate) activity: Option<Activity>,
    pub(crate) last_activity: DateTime<Local>,
}

/// Detail fields collected for the UI detail view, not used in domain logic.
#[derive(Debug, Clone)]
pub struct AgentDetail {
    pub(crate) subagents: Vec<Subagent>,
    pub(crate) recent_events: VecDeque<ActivityEvent>,
    pub(crate) todos: Vec<TodoItem>,
}

// ── Activity event types ────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActivityEventKind {
    ToolCall,
    SubagentSpawned,
    SubagentCompleted,
    StatusChanged,
    Message,
}

#[derive(Debug, Clone)]
pub struct ActivityEvent {
    pub timestamp: DateTime<Local>,
    pub kind: ActivityEventKind,
    pub description: String,
}

// ── Agent (composed) ─────────────────────────────────────────

/// The authoritative representation of a detected coding agent.
///
/// Composed of three sub-structs:
/// - [`AgentIdent`] — stable identity fields
/// - [`AgentLifecycle`] — mutable lifecycle state
/// - [`AgentDetail`] — UI-facing detail
///
/// Delegation methods forward to the sub-struct fields so callers
/// don't need to reach through the composition.
#[derive(Debug, Clone)]
pub struct Agent {
    pub(crate) ident: AgentIdent,
    pub(crate) lifecycle: AgentLifecycle,
    pub(crate) detail: AgentDetail,
}

// ── Construction ─────────────────────────────────────────────

impl Agent {
    /// Creates a new `Agent` from its sub-structs.
    ///
    /// Prefer struct-literal construction within the crate. This constructor
    /// exists for integration tests that live outside the crate boundary.
    pub fn new(ident: AgentIdent, lifecycle: AgentLifecycle, detail: AgentDetail) -> Self {
        Self {
            ident,
            lifecycle,
            detail,
        }
    }
}

impl AgentIdent {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: AgentId,
        pid: Option<u32>,
        session_id: Option<String>,
        alias: Option<String>,
        cwd: Option<PathBuf>,
        branch: Option<String>,
        is_worktree: bool,
        started_at: DateTime<Local>,
    ) -> Self {
        Self {
            id,
            pid,
            session_id,
            alias,
            cwd,
            branch,
            is_worktree,
            started_at,
        }
    }
}

impl AgentLifecycle {
    pub fn new(
        status: PrimaryStatus,
        normalized: NormalizedState,
        activity: Option<Activity>,
        last_activity: DateTime<Local>,
    ) -> Self {
        Self {
            status,
            normalized,
            activity,
            last_activity,
        }
    }
}

impl AgentDetail {
    pub fn new(
        subagents: Vec<Subagent>,
        recent_events: VecDeque<ActivityEvent>,
        todos: Vec<TodoItem>,
    ) -> Self {
        Self {
            subagents,
            recent_events,
            todos,
        }
    }
}

// ── Delegation: identity ─────────────────────────────────────

impl Agent {
    pub fn id(&self) -> &AgentId {
        &self.ident.id
    }

    pub fn pid(&self) -> Option<u32> {
        self.ident.pid
    }

    pub fn session_id(&self) -> Option<&str> {
        self.ident.session_id.as_deref()
    }

    pub fn alias(&self) -> Option<&str> {
        self.ident.alias.as_deref()
    }

    pub fn cwd(&self) -> Option<&Path> {
        self.ident.cwd.as_deref()
    }

    pub fn branch(&self) -> Option<&str> {
        self.ident.branch.as_deref()
    }

    pub fn is_worktree(&self) -> bool {
        self.ident.is_worktree
    }

    pub fn started_at(&self) -> DateTime<Local> {
        self.ident.started_at
    }
}

// ── Delegation: lifecycle ────────────────────────────────────

impl Agent {
    pub fn status(&self) -> PrimaryStatus {
        self.lifecycle.status
    }

    pub fn normalized(&self) -> &NormalizedState {
        &self.lifecycle.normalized
    }

    pub fn activity(&self) -> Option<Activity> {
        self.lifecycle.activity
    }

    pub fn last_activity(&self) -> DateTime<Local> {
        self.lifecycle.last_activity
    }
}

// ── Delegation: detail ───────────────────────────────────────

impl Agent {
    pub fn subagents(&self) -> &[Subagent] {
        &self.detail.subagents
    }

    pub fn recent_events(&self) -> &VecDeque<ActivityEvent> {
        &self.detail.recent_events
    }

    pub fn todos(&self) -> &[TodoItem] {
        &self.detail.todos
    }
}

// ── Derived helpers ──────────────────────────────────────────

impl Agent {
    pub fn elapsed(&self) -> chrono::Duration {
        Local::now() - self.ident.started_at
    }

    pub fn idle_duration(&self) -> chrono::Duration {
        Local::now() - self.lifecycle.last_activity
    }

    pub fn display_name(&self) -> &str {
        self.ident
            .alias
            .as_deref()
            .unwrap_or(self.ident.id.tool().as_str())
    }
}

// ── Mutation: identity ───────────────────────────────────────

impl Agent {
    pub fn merge_pid(&mut self, pid: Option<u32>) {
        self.ident.pid = pid.or(self.ident.pid);
    }

    pub fn set_cwd(&mut self, cwd: PathBuf) {
        self.ident.cwd = Some(cwd);
    }

    pub fn set_id(&mut self, id: AgentId) {
        self.ident.id = id;
    }

    pub fn set_session_id(&mut self, session_id: Option<String>) {
        if session_id.is_some() {
            self.ident.session_id = session_id;
        }
    }

    pub fn set_branch(&mut self, branch: Option<String>) {
        if branch.is_some() {
            self.ident.branch = branch;
        }
    }

    pub fn set_is_worktree(&mut self, is_worktree: bool) {
        self.ident.is_worktree = is_worktree;
    }

    pub fn set_alias(&mut self, alias: Option<String>) {
        self.ident.alias = alias;
    }

    pub fn clamp_started_at(&mut self, real_start: DateTime<Local>) {
        if real_start < self.ident.started_at {
            self.ident.started_at = real_start;
        }
    }
}

// ── Mutation: lifecycle ──────────────────────────────────────

impl Agent {
    pub fn set_status(&mut self, status: PrimaryStatus) {
        self.lifecycle.status = status;
    }

    pub fn set_activity(&mut self, activity: Option<Activity>) {
        self.lifecycle.activity = activity;
    }

    pub fn set_normalized(&mut self, state: NormalizedState) {
        self.lifecycle.normalized = state;
    }

    pub fn touch(&mut self, now: DateTime<Local>) {
        self.lifecycle.last_activity = now;
    }
}

// ── Mutation: detail ─────────────────────────────────────────

impl Agent {
    pub fn push_subagent(&mut self, subagent: Subagent) {
        self.detail.subagents.push(subagent);
    }

    pub fn set_subagents(&mut self, subagents: Vec<Subagent>) {
        self.detail.subagents = subagents;
    }

    pub fn set_todos(&mut self, todos: Vec<TodoItem>) {
        self.detail.todos = todos;
    }

    pub fn subagents_mut(&mut self) -> &mut Vec<Subagent> {
        &mut self.detail.subagents
    }

    pub fn recent_events_mut(&mut self) -> &mut VecDeque<ActivityEvent> {
        &mut self.detail.recent_events
    }

    pub fn clear_subagent_events(&mut self) {
        self.detail.recent_events.retain(|event| {
            !matches!(
                event.kind,
                ActivityEventKind::SubagentSpawned | ActivityEventKind::SubagentCompleted
            )
        });
    }
}
