use std::path::PathBuf;

use chrono::{DateTime, Local};

use crate::types::{Activity, AgentId, PrimaryStatus, Subagent, TodoItem};

#[derive(Debug, Clone)]
pub enum AgentEvent {
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
        state: crate::types::NormalizedState,
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
    ToolCallStarted {
        id: AgentId,
        pid: Option<u32>,
        tool_name: String,
        args_summary: Option<String>,
    },
    ToolCallCompleted {
        id: AgentId,
        pid: Option<u32>,
        tool_name: String,
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
    CodexQuotaRefreshed {
        profile_id: String,
        quota: Option<crate::codex_profiles::quota::QuotaSnapshot>,
    },
    ClaudeQuotaRefreshed {
        profile_id: String,
        quota: Option<crate::codex_profiles::quota::QuotaSnapshot>,
    },
}
