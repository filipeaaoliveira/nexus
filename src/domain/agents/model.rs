use std::collections::VecDeque;
use std::path::PathBuf;

use chrono::{DateTime, Local};

use super::activity::{Activity, ActivityEvent, Subagent, TodoItem};
use super::ids::AgentId;
use super::status::{NormalizedState, PrimaryStatus};

#[derive(Debug, Clone)]
pub struct Agent {
    pub id: AgentId,
    pub pid: Option<u32>,
    pub status: PrimaryStatus,
    pub normalized: NormalizedState,
    pub activity: Option<Activity>,
    pub cwd: PathBuf,
    pub branch: Option<String>,
    pub is_worktree: bool,
    pub started_at: DateTime<Local>,
    pub last_activity: DateTime<Local>,
    pub subagents: Vec<Subagent>,
    pub recent_events: VecDeque<ActivityEvent>,
    pub session_id: Option<String>,
    pub alias: Option<String>,
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
