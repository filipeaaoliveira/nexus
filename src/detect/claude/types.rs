use std::path::PathBuf;

use serde_json::Value;

use crate::domain::agents::activity::TodoItem;
use crate::domain::agents::status::{NormalizedState, PrimaryStatus};

/// A parsed entry from a Claude transcript JSONL file.
#[derive(Debug, Clone)]
pub enum TranscriptEntry {
    User {
        timestamp: String,
        content: String,
    },
    Assistant {
        timestamp: String,
        content: String,
    },
    ToolUse {
        timestamp: String,
        tool_name: String,
        tool_input: serde_json::Map<String, Value>,
    },
    ToolResult {
        timestamp: String,
        tool_name: String,
    },
    /// Emitted from `system` entries with `subtype: "turn_duration"`.
    /// Signals that the agent completed a turn and is waiting for user input.
    TurnCompleted {
        timestamp: String,
    },
    /// Emitted from `system` entries with `subtype: "api_error"`.
    /// Signals that the API returned an error and the agent is retrying.
    ApiError {
        timestamp: String,
        retry_attempt: Option<u32>,
        max_retries: Option<u32>,
    },
    /// Emitted from `system` entries with `subtype: "compact_boundary"`.
    /// Signals that context compaction is in progress.
    CompactBoundary {
        timestamp: String,
    },
    /// Emitted when an assistant message has a terminal `stop_reason` other
    /// than `end_turn` or `tool_use` — specifically `max_tokens`, `refusal`,
    /// or `pause_turn`. All three are treated as a blocked state in this
    /// observer context. Note: `pause_turn` is a Claude-side pause that may be
    /// internally resumable via the API, but from the perspective of a passive
    /// transcript observer there is no meaningful distinction from a hard block.
    TurnBlocked {
        timestamp: String,
        stop_reason: String,
    },
    /// Emitted when a user entry's top-level `content` string contains local
    /// command injection metadata: `<local-command-caveat>`, `<command-name>`,
    /// or `<local-command-stdout>`.  Claude Code injects these entries when a
    /// slash command (e.g. `/compact`) is executed; they record the command
    /// name and its stdout for context but carry no user intent.  They must be
    /// excluded from state inference so that `/compact` does not drive the
    /// session to "working" or "unknown".
    LocalCommand {
        timestamp: String,
    },
}

/// Status inferred from the last transcript entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InferredStatus {
    Running,
    Waiting,
    ToolCall,
    Unknown,
}

/// Information extracted from a Claude project session JSONL file.
#[derive(Debug, Clone, Default)]
pub struct ProjectSessionInfo {
    pub session_id: Option<String>,
    pub branch: Option<String>,
    pub cwd: Option<PathBuf>,
    pub model: Option<String>,
    pub has_internal_initiator: bool,
    pub started_at: Option<String>,
}

/// An entry from `~/.claude/sessions/<pid>.json`.
/// Written by Claude Code while a session is live; cleaned up on exit.
#[derive(Debug, Clone)]
pub struct SessionRegistryEntry {
    pub pid: u32,
    pub session_id: String,
    /// Working directory reported by Claude Code.  Currently used only in
    /// tests; retained here for future cross-validation against the CWD
    /// derived from the project session file path.
    pub cwd: PathBuf,
    pub kind: String,
}

/// Per-session emit state to avoid duplicate events.
#[derive(Debug, Clone, Default)]
pub(super) struct SessionEmitState {
    pub last_status: Option<PrimaryStatus>,
    pub last_normalized_state: Option<NormalizedState>,
    pub last_tool_call: Option<String>,
    pub emitted_subagent_tools: Vec<String>,
    pub session_info_sent: bool,
    pub entry_count: usize,
    pub last_mtime: Option<std::time::SystemTime>,
    pub pid: Option<u32>,
    pub last_todos: Vec<TodoItem>,
    pub last_entries: Vec<TranscriptEntry>,
}
