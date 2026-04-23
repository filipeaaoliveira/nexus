use std::str::FromStr;

use chrono::{DateTime, Utc};

use super::status::{NormalizedReason, PrimaryStatus};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
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
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum TodoStatus {
    #[default]
    Pending,
    InProgress,
    Completed,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParseTodoStatusError;

impl std::fmt::Display for ParseTodoStatusError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("unknown todo status")
    }
}

impl std::error::Error for ParseTodoStatusError {}

impl FromStr for TodoStatus {
    type Err = ParseTodoStatusError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "pending" => Ok(Self::Pending),
            "in_progress" => Ok(Self::InProgress),
            "completed" => Ok(Self::Completed),
            "cancelled" => Ok(Self::Cancelled),
            _ => Err(ParseTodoStatusError),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum TodoPriority {
    High,
    Medium,
    Low,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParseTodoPriorityError;

impl std::fmt::Display for ParseTodoPriorityError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("unknown todo priority")
    }
}

impl std::error::Error for ParseTodoPriorityError {}

impl TodoPriority {
    /// Default priority used when parsing fails.
    pub const DEFAULT: Self = Self::Medium;
}

impl FromStr for TodoPriority {
    type Err = ParseTodoPriorityError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "high" => Ok(Self::High),
            "medium" => Ok(Self::Medium),
            "low" => Ok(Self::Low),
            _ => Err(ParseTodoPriorityError),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TodoItem {
    pub content: String,
    pub status: TodoStatus,
    pub priority: TodoPriority,
}
