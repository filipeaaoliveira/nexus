use std::path::PathBuf;

use chrono::{DateTime, Local};

use crate::types::{Activity, AgentId, PrimaryStatus, Subagent, TodoItem};

#[derive(Debug, Clone)]
pub enum AgentEvent {
    Discovered {
        id: AgentId,
        cwd: PathBuf,
        started_at: Option<DateTime<Local>>,
    },
    StatusChanged {
        id: AgentId,
        old: PrimaryStatus,
        new: PrimaryStatus,
    },
    ActivityChanged {
        id: AgentId,
        activity: Option<Activity>,
    },
    SubagentSpawned {
        id: AgentId,
        subagent: Subagent,
    },
    SubagentCompleted {
        id: AgentId,
        subagent_name: String,
    },
    ToolCallStarted {
        id: AgentId,
        tool_name: String,
        args_summary: Option<String>,
    },
    ToolCallCompleted {
        id: AgentId,
        tool_name: String,
    },
    ProcessExited {
        id: AgentId,
    },
    TodosUpdated {
        id: AgentId,
        todos: Vec<TodoItem>,
    },
    SessionInfo {
        id: AgentId,
        session_id: String,
        branch: Option<String>,
        is_worktree: bool,
        started_at: Option<DateTime<Local>>,
    },
}
