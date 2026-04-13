use std::path::PathBuf;

use chrono::{DateTime, Local};

use crate::domain::agents::activity::{Activity, Subagent, TodoItem};
use crate::domain::agents::ids::AgentId;
use crate::domain::agents::status::{NormalizedState, PrimaryStatus};
use crate::domain::system::DetectorKind;

#[derive(Debug, Clone)]
pub enum AgentEvent {
    DetectorHealthChanged {
        detector: DetectorKind,
        healthy: bool,
        message: String,
    },
    Discovered {
        id: AgentId,
        pid: Option<u32>,
        cwd: PathBuf,
        started_at: Option<DateTime<Local>>,
    },
    StatusChanged {
        id: AgentId,
        pid: Option<u32>,
        old: PrimaryStatus,
        new: PrimaryStatus,
    },
    ActivityChanged {
        id: AgentId,
        pid: Option<u32>,
        activity: Option<Activity>,
    },
    NormalizedStateChanged {
        id: AgentId,
        pid: Option<u32>,
        state: NormalizedState,
    },
    SubagentSpawned {
        id: AgentId,
        pid: Option<u32>,
        subagent: Subagent,
    },
    SubagentCompleted {
        id: AgentId,
        pid: Option<u32>,
        subagent_name: String,
    },
    SubagentsUpdated {
        id: AgentId,
        pid: Option<u32>,
        subagents: Vec<Subagent>,
    },
    ToolCallStarted {
        id: AgentId,
        pid: Option<u32>,
        tool_name: String,
        args_summary: Option<String>,
    },
    ToolCallCompleted {
        id: AgentId,
        pid: Option<u32>,
    },
    ProcessExited {
        id: AgentId,
        pid: Option<u32>,
    },
    TodosUpdated {
        id: AgentId,
        pid: Option<u32>,
        todos: Vec<TodoItem>,
    },
    SessionInfo {
        id: AgentId,
        pid: Option<u32>,
        session_id: String,
        branch: Option<String>,
        is_worktree: bool,
        started_at: Option<DateTime<Local>>,
    },
}
