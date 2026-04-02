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

// ── AgentIdentity / AgentId ────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AgentIdentity {
    Process { tool: AgentTool, pid: u32 },
    Session { tool: AgentTool, session_id: String },
}

impl AgentIdentity {
    pub fn tool(&self) -> AgentTool {
        match self {
            Self::Process { tool, .. } | Self::Session { tool, .. } => *tool,
        }
    }

    pub fn pid(&self) -> Option<u32> {
        match self {
            Self::Process { pid, .. } => Some(*pid),
            Self::Session { .. } => None,
        }
    }

    pub fn session_id(&self) -> Option<&str> {
        match self {
            Self::Process { .. } => None,
            Self::Session { session_id, .. } => Some(session_id.as_str()),
        }
    }

    pub fn is_process(&self) -> bool {
        matches!(self, Self::Process { .. })
    }

    pub fn is_session(&self) -> bool {
        matches!(self, Self::Session { .. })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AgentId {
    pub identity: AgentIdentity,
}

impl AgentId {
    pub fn new(tool: AgentTool, pid: u32) -> Self {
        Self {
            identity: AgentIdentity::Process { tool, pid },
        }
    }

    pub fn session(tool: AgentTool, session_id: impl Into<String>) -> Self {
        Self {
            identity: AgentIdentity::Session {
                tool,
                session_id: session_id.into(),
            },
        }
    }

    pub fn tool(&self) -> AgentTool {
        self.identity.tool()
    }

    pub fn pid(&self) -> Option<u32> {
        self.identity.pid()
    }

    pub fn session_id(&self) -> Option<&str> {
        self.identity.session_id()
    }

    pub fn is_process(&self) -> bool {
        self.identity.is_process()
    }

    pub fn is_session(&self) -> bool {
        self.identity.is_session()
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

// ── NormalizedStatus / NormalizedReason ────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum NormalizedStatus {
    Unknown = 0,
    Done = 1,
    Working = 2,
    Blocked = 3,
    NeedsInput = 4,
}

impl NormalizedStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            NormalizedStatus::Unknown => "unknown",
            NormalizedStatus::Done => "done",
            NormalizedStatus::Working => "working",
            NormalizedStatus::Blocked => "blocked",
            NormalizedStatus::NeedsInput => "needs input",
        }
    }
}

impl std::fmt::Display for NormalizedStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum NormalizedReason {
    ToolCall,
    Subagent,
    Retrying,
    Approval,
    Decision,
    Review,
    Error,
    ReadyForNextTask,
    Custom(String),
}

impl NormalizedReason {
    pub fn as_str(&self) -> &str {
        match self {
            NormalizedReason::ToolCall => "tool call",
            NormalizedReason::Subagent => "subagent",
            NormalizedReason::Retrying => "retrying",
            NormalizedReason::Approval => "approval",
            NormalizedReason::Decision => "decision",
            NormalizedReason::Review => "review",
            NormalizedReason::Error => "error",
            NormalizedReason::ReadyForNextTask => "ready for next task",
            NormalizedReason::Custom(reason) => reason.as_str(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizedState {
    pub status: NormalizedStatus,
    pub reason: NormalizedReason,
}

impl NormalizedState {
    pub fn new(status: NormalizedStatus, reason: NormalizedReason) -> Self {
        Self { status, reason }
    }
}

// ── Activity ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Activity {
    Generating,
    ToolCall,
    Idle,
    Thinking,
    Compacting,
}

impl Activity {
    pub fn as_str(&self) -> &'static str {
        match self {
            Activity::Generating => "generating",
            Activity::ToolCall => "tool call",
            Activity::Idle => "idle",
            Activity::Thinking => "thinking",
            Activity::Compacting => "compacting",
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

// ── Todos ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TodoStatus {
    Pending,
    InProgress,
    Completed,
    Cancelled,
}

impl TodoStatus {
    pub fn parse_str(s: &str) -> Self {
        match s {
            "pending" => Self::Pending,
            "in_progress" => Self::InProgress,
            "completed" => Self::Completed,
            "cancelled" => Self::Cancelled,
            _ => Self::Pending,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TodoPriority {
    High,
    Medium,
    Low,
}

impl TodoPriority {
    pub fn parse_str(s: &str) -> Self {
        match s {
            "high" => Self::High,
            "medium" => Self::Medium,
            "low" => Self::Low,
            _ => Self::Medium,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TodoItem {
    pub content: String,
    pub status: TodoStatus,
    pub priority: TodoPriority,
}

// ── Agent ──────────────────────────────────────────────────────────

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

// ── Timestamp conversion helpers ──────────────────────────────────

/// Convert epoch milliseconds to local DateTime. Returns None for 0 or invalid values.
pub fn from_epoch_millis(ms: i64) -> Option<DateTime<Local>> {
    if ms == 0 {
        return None;
    }
    chrono::DateTime::from_timestamp_millis(ms).map(|dt| dt.with_timezone(&Local))
}

/// Convert epoch seconds to local DateTime. Returns None for 0 or invalid values.
pub fn from_epoch_secs(secs: i64) -> Option<DateTime<Local>> {
    if secs == 0 {
        return None;
    }
    chrono::DateTime::from_timestamp(secs, 0).map(|dt| dt.with_timezone(&Local))
}

/// Parse an RFC3339 timestamp string to local DateTime. Returns None for invalid input.
pub fn from_rfc3339(s: &str) -> Option<DateTime<Local>> {
    chrono::DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|dt| dt.with_timezone(&Local))
}
