use std::collections::VecDeque;
use std::path::PathBuf;

use chrono::{DateTime, Local};

// ── AgentTool ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AgentTool {
    OpenCode,
    Claude,
    Codex,
}

impl AgentTool {
    pub fn as_str(&self) -> &'static str {
        match self {
            AgentTool::OpenCode => "opencode",
            AgentTool::Claude => "claude",
            AgentTool::Codex => "codex",
        }
    }
}

impl std::fmt::Display for AgentTool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

// ── AgentId ────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AgentId {
    pub tool: AgentTool,
    pub pid: u32,
}

impl AgentId {
    pub fn new(tool: AgentTool, pid: u32) -> Self {
        Self { tool, pid }
    }
}

// ── PrimaryStatus ──────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PrimaryStatus {
    Waiting = 0,
    Running = 1,
    Error = 2,
    Done = 3,
    Exited = 4,
}

impl PrimaryStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            PrimaryStatus::Waiting => "WAITING",
            PrimaryStatus::Running => "RUNNING",
            PrimaryStatus::Error => "ERROR",
            PrimaryStatus::Done => "DONE",
            PrimaryStatus::Exited => "EXITED",
        }
    }
}

impl std::fmt::Display for PrimaryStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

// ── Activity ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Activity {
    Generating,
    ToolCall,
    Idle,
    Thinking,
}

impl Activity {
    pub fn as_str(&self) -> &'static str {
        match self {
            Activity::Generating => "generating",
            Activity::ToolCall => "tool call",
            Activity::Idle => "idle",
            Activity::Thinking => "thinking",
        }
    }
}

impl std::fmt::Display for Activity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

// ── ActivityEventKind ──────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActivityEventKind {
    ToolCall,
    SubagentSpawned,
    SubagentCompleted,
    StatusChanged,
    Message,
}

// ── ActivityEvent ──────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ActivityEvent {
    pub timestamp: DateTime<Local>,
    pub kind: ActivityEventKind,
    pub description: String,
}

// ── Subagent ───────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Subagent {
    pub name: String,
    pub status: PrimaryStatus,
    pub description: Option<String>,
    pub started_at: DateTime<Local>,
    pub ended_at: Option<DateTime<Local>>,
}

// ── Agent ──────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Agent {
    pub id: AgentId,
    pub status: PrimaryStatus,
    pub activity: Option<Activity>,
    pub cwd: PathBuf,
    pub branch: Option<String>,
    pub is_worktree: bool,
    pub started_at: DateTime<Local>,
    pub last_activity: DateTime<Local>,
    pub subagents: Vec<Subagent>,
    pub recent_events: VecDeque<ActivityEvent>,
    pub session_id: Option<String>,
}

impl Agent {
    pub fn elapsed(&self) -> chrono::Duration {
        Local::now() - self.started_at
    }

    pub fn idle_duration(&self) -> chrono::Duration {
        Local::now() - self.last_activity
    }

    pub fn display_name(&self) -> &str {
        self.id.tool.as_str()
    }
}

// ── SortMode ───────────────────────────────────────────────────────

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

// ── Duration formatting ───────────────────────────────────────────

/// Format a duration for display: "5s", "3m12s", "2h15m".
pub fn format_duration(d: chrono::Duration) -> String {
    let secs = d.num_seconds().max(0);
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m{}s", secs / 60, secs % 60)
    } else {
        format!("{}h{}m", secs / 3600, (secs % 3600) / 60)
    }
}

/// Format a duration in short form: "5s", "3m", "2h".
pub fn format_duration_short(d: chrono::Duration) -> String {
    let secs = d.num_seconds().max(0);
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m", secs / 60)
    } else {
        format!("{}h", secs / 3600)
    }
}
