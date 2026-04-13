use std::collections::VecDeque;
use std::path::PathBuf;

use chrono::{DateTime, Local};

use super::activity::{Activity, ActivityEvent, Subagent, TodoItem};
use super::ids::AgentId;
use super::status::{NormalizedState, PrimaryStatus};

/// The authoritative representation of a detected coding agent.
///
/// # Field groups
///
/// **Identity** — stable across the agent's lifetime:
///   `id`, `pid`, `session_id`, `alias`, `cwd`, `branch`, `is_worktree`, `started_at`
///
/// **Lifecycle state** — changes as the agent progresses:
///   `status`, `normalized`, `activity`, `last_activity`
///
/// **Detail / UI-facing** — collected for the detail view, not used in domain logic:
///   `subagents`, `recent_events`, `todos`
///
/// Display formatting lives in [`super::super::app::projection::AgentView`],
/// not here.
#[derive(Debug, Clone)]
pub struct Agent {
    // ── Identity ──────────────────────────────────────────────
    pub id: AgentId,
    pub pid: Option<u32>,
    pub session_id: Option<String>,
    pub alias: Option<String>,
    pub cwd: Option<PathBuf>,
    pub branch: Option<String>,
    pub is_worktree: bool,
    pub started_at: DateTime<Local>,

    // ── Lifecycle state ──────────────────────────────────────
    pub status: PrimaryStatus,
    pub normalized: NormalizedState,
    pub activity: Option<Activity>,
    pub last_activity: DateTime<Local>,

    // ── Detail / UI-facing ───────────────────────────────────
    pub subagents: Vec<Subagent>,
    pub recent_events: VecDeque<ActivityEvent>,
    pub todos: Vec<TodoItem>,
}

impl Agent {
    pub fn elapsed(&self) -> chrono::Duration {
        Local::now() - self.started_at
    }

    pub fn idle_duration(&self) -> chrono::Duration {
        Local::now() - self.last_activity
    }

    pub fn display_name(&self) -> &str {
        self.alias.as_deref().unwrap_or(self.id.tool().as_str())
    }
}

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
