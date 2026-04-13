use std::collections::HashSet;
use std::path::PathBuf;

use crate::domain::agents::activity::TodoItem;
use crate::domain::agents::status::{NormalizedState, PrimaryStatus};

/// Parsed message data from OpenCode's SQLite `message.data` JSON.
#[allow(dead_code)] // parsed message rows intentionally retain optional provider metadata
#[derive(Debug, Clone)]
pub struct MessageData {
    pub role: String,
    pub agent: Option<String>,
    pub model_id: Option<String>,
    pub provider_id: Option<String>,
}

/// Parsed assistant message data from OpenCode's SQLite `message.data` JSON.
#[derive(Debug, Clone)]
pub struct AssistantMessageData {
    pub text: Option<String>,
    pub error: Option<String>,
}

/// Parsed part data from OpenCode's SQLite `part.data` JSON.
#[derive(Debug, Clone)]
pub struct PartData {
    pub part_type: String,
    pub tool_name: Option<String>,
    pub tool_status: Option<String>,
    pub tool_input_summary: Option<String>,
    pub time_created: i64,
}

/// An OpenCode session from the SQLite database.
#[allow(dead_code)] // sqlite row mirror keeps extra columns that are useful in tests and diagnostics
#[derive(Debug, Clone)]
pub struct OpenCodeSession {
    pub id: String,
    pub title: Option<String>,
    pub directory: Option<PathBuf>,
    pub parent_id: Option<String>,
    pub time_created: i64,
    pub time_updated: i64,
    pub time_archived: Option<i64>,
    pub branch: Option<String>,
}

/// State tracked per session to avoid emitting duplicate events.
#[derive(Debug, Clone, Default)]
pub struct SessionEmitState {
    pub last_status: Option<PrimaryStatus>,
    pub last_normalized_state: Option<NormalizedState>,
    pub last_tool_call: Option<String>,
    /// Tracks the time_created of the most recent part we saw, so we can
    /// detect new activity even when the tool name/status hasn't changed.
    pub last_part_time: i64,
    pub emitted_subagent_ids: HashSet<String>,
    pub completed_subagent_ids: HashSet<String>,
    pub session_info_sent: bool,
    pub last_todos: Vec<TodoItem>,
}
