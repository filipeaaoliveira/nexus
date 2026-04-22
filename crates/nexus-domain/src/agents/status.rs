#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
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
#[non_exhaustive]
pub enum NormalizedReason {
    ToolCall,
    Subagent,
    Retrying,
    Approval,
    Decision,
    Review,
    Error,
    Compacting,
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
            NormalizedReason::Compacting => "compacting",
            NormalizedReason::ReadyForNextTask => "ready for next task",
            NormalizedReason::Custom(reason) => reason.as_str(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizedState {
    pub status: NormalizedStatus,
    pub reason: Option<NormalizedReason>,
}

impl NormalizedState {
    pub fn new(status: NormalizedStatus, reason: Option<NormalizedReason>) -> Self {
        Self { status, reason }
    }
}
