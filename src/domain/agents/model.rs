use std::collections::VecDeque;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Local};

use super::activity::{Activity, ActivityEvent, Subagent, TodoItem};
use super::ids::AgentId;
use super::status::{NormalizedState, PrimaryStatus};

// ── Sub-structs ──────────────────────────────────────────────

/// Stable identity fields that remain constant across the agent's lifetime.
#[derive(Debug, Clone)]
pub struct AgentIdent {
    pub id: AgentId,
    pub pid: Option<u32>,
    pub session_id: Option<String>,
    pub alias: Option<String>,
    pub cwd: Option<PathBuf>,
    pub branch: Option<String>,
    pub is_worktree: bool,
    pub started_at: DateTime<Local>,
}

/// Mutable lifecycle state that changes as the agent progresses.
#[derive(Debug, Clone)]
pub struct AgentLifecycle {
    pub status: PrimaryStatus,
    pub normalized: NormalizedState,
    pub activity: Option<Activity>,
    pub last_activity: DateTime<Local>,
}

/// Detail fields collected for the UI detail view, not used in domain logic.
#[derive(Debug, Clone)]
pub struct AgentDetail {
    pub subagents: Vec<Subagent>,
    pub recent_events: VecDeque<ActivityEvent>,
    pub todos: Vec<TodoItem>,
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
    pub ident: AgentIdent,
    pub lifecycle: AgentLifecycle,
    pub detail: AgentDetail,
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

// ── SortMode ─────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortMode {
    Urgency,
    Name,
    Elapsed,
}

impl SortMode {
    pub fn next(&self) -> SortMode {
        match self {
            SortMode::Urgency => SortMode::Name,
            SortMode::Name => SortMode::Elapsed,
            SortMode::Elapsed => SortMode::Urgency,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            SortMode::Urgency => "urgency",
            SortMode::Name => "name",
            SortMode::Elapsed => "elapsed",
        }
    }
}

impl std::fmt::Display for SortMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}
