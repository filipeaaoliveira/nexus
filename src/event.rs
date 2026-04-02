use std::path::PathBuf;

use chrono::{DateTime, Local};

use crate::types::{Activity, AgentTool, PrimaryStatus, Subagent, TodoItem};

#[derive(Debug, Clone)]
pub enum AgentEvent {
    Discovered {
        tool: AgentTool,
        pid: u32,
        cwd: PathBuf,
        started_at: Option<DateTime<Local>>,
    },
    StatusChanged {
        pid: u32,
        tool: AgentTool,
        old: PrimaryStatus,
        new: PrimaryStatus,
    },
    ActivityChanged {
        pid: u32,
        tool: AgentTool,
        activity: Option<Activity>,
    },
    SubagentSpawned {
        pid: u32,
        tool: AgentTool,
        subagent: Subagent,
    },
    SubagentCompleted {
        pid: u32,
        tool: AgentTool,
        subagent_name: String,
    },
    ToolCallStarted {
        pid: u32,
        tool: AgentTool,
        tool_name: String,
        args_summary: Option<String>,
    },
    ToolCallCompleted {
        pid: u32,
        tool: AgentTool,
        tool_name: String,
    },
    ProcessExited {
        pid: u32,
        tool: AgentTool,
    },
    TodosUpdated {
        pid: u32,
        tool: AgentTool,
        todos: Vec<TodoItem>,
    },
    SessionInfo {
        pid: u32,
        tool: AgentTool,
        session_id: String,
        branch: Option<String>,
        is_worktree: bool,
        started_at: Option<DateTime<Local>>,
    },
}
