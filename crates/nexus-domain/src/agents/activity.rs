use chrono::{DateTime, Local};

use super::status::{NormalizedReason, PrimaryStatus};

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

#[derive(Debug, Clone, PartialEq)]
pub struct Subagent {
    pub name: String,
    pub status: PrimaryStatus,
    pub reason: Option<NormalizedReason>,
    pub description: Option<String>,
    pub started_at: DateTime<Local>,
    pub ended_at: Option<DateTime<Local>>,
}

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
