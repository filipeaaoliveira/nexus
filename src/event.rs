use std::path::PathBuf;

use chrono::{DateTime, Utc};

use nexus_domain::agents::activity::{Activity, Subagent, TodoItem};
use nexus_domain::agents::ids::AgentId;
use nexus_domain::agents::status::{NormalizedState, PrimaryStatus};
use nexus_domain::system::DetectorKind;

/// Identifies an agent instance across events.
#[derive(Debug, Clone, PartialEq)]
pub struct AgentRef {
    pub id: AgentId,
    pub pid: Option<u32>,
}

impl AgentRef {
    pub fn new(id: AgentId, pid: Option<u32>) -> Self {
        Self { id, pid }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum AgentEvent {
    DetectorHealthChanged {
        detector: DetectorKind,
        healthy: bool,
        message: String,
    },
    Discovered {
        agent: AgentRef,
        cwd: PathBuf,
        started_at: Option<DateTime<Utc>>,
    },
    StatusChanged {
        agent: AgentRef,
        old: PrimaryStatus,
        new: PrimaryStatus,
    },
    ActivityChanged {
        agent: AgentRef,
        activity: Option<Activity>,
    },
    NormalizedStateChanged {
        agent: AgentRef,
        state: NormalizedState,
    },
    SubagentSpawned {
        agent: AgentRef,
        subagent: Subagent,
    },
    SubagentCompleted {
        agent: AgentRef,
        subagent_name: String,
    },
    SubagentsUpdated {
        agent: AgentRef,
        subagents: Vec<Subagent>,
    },
    ToolCallStarted {
        agent: AgentRef,
        tool_name: String,
        args_summary: Option<String>,
    },
    ToolCallCompleted {
        agent: AgentRef,
    },
    ProcessExited {
        agent: AgentRef,
    },
    TodosUpdated {
        agent: AgentRef,
        todos: Vec<TodoItem>,
    },
    SessionInfo {
        agent: AgentRef,
        session_id: Option<String>,
        branch: Option<String>,
        is_worktree: bool,
        started_at: Option<DateTime<Utc>>,
    },
}
