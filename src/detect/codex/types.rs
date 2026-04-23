use std::path::PathBuf;

use chrono::{DateTime, Utc};

use nexus_domain::agents::activity::TodoItem;
use nexus_domain::agents::ids::AgentId;
use nexus_domain::agents::status::{NormalizedReason, PrimaryStatus};
use nexus_domain::system::health::CodexBackendHealth;

// ── Types ─────────────────────────────────────────────────────────

/// A parsed event from a Codex rollout JSONL file.
#[allow(dead_code)] // rollout parsing keeps a small schema mirror for future heuristics and tests
#[derive(Debug, Clone)]
pub struct RolloutEvent {
    pub timestamp: String,
    pub event_type: String,
    pub payload: serde_json::Value,
}

/// Session metadata extracted from a rollout's `session_meta` event.
#[allow(dead_code)] // retained schema fields are intentionally broader than current inference needs
#[derive(Debug, Clone)]
pub struct SessionMeta {
    pub id: String,
    pub cwd: String,
    pub branch: Option<String>,
    pub source: String,
    pub model_provider: String,
}

/// A subagent spawn extracted from a `collab_agent_spawn_end` event.
#[allow(dead_code)] // prompt/role are preserved for richer subagent labeling and tests
#[derive(Debug, Clone)]
pub struct SubagentSpawn {
    pub nickname: String,
    pub role: String,
    pub thread_id: String,
    pub prompt: String,
}

/// A tool call extracted from a `function_call` response item.
#[derive(Debug, Clone)]
pub struct ToolCallInfo {
    pub name: String,
    pub arguments: String,
}

/// Inferred status of a Codex rollout based on its events.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodexInferredStatus {
    Running,
    Done,
    Error,
    Aborted,
    Unknown,
}

/// An entry from the Codex session index JSONL file.
#[allow(dead_code)] // the index format carries more metadata than the current production path uses
#[derive(Debug, Clone)]
pub struct SessionIndexEntry {
    pub id: String,
    pub thread_name: String,
    pub updated_at: String,
}

/// Parsed source field info — distinguishes CLI launches from subagent spawns.
#[allow(dead_code)] // tests and future reconciliation may inspect the full parsed source payload
#[derive(Debug, Clone)]
pub struct SourceInfo {
    pub is_subagent: bool,
    pub parent_thread_id: Option<String>,
    pub agent_nickname: Option<String>,
    pub agent_role: Option<String>,
}

/// A Codex thread from the SQLite database.
#[allow(dead_code)] // sqlite row mirror intentionally retains non-critical columns for diagnostics/tests
#[derive(Debug, Clone)]
pub struct CodexThread {
    pub id: String,
    pub title: Option<String>,
    pub cwd: Option<String>,
    pub source: Option<String>,
    pub model: Option<String>,
    pub git_branch: Option<String>,
    pub agent_nickname: Option<String>,
    pub agent_role: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub archived: i32,
    pub tokens_used: Option<i64>,
    pub rollout_path: Option<String>,
}

/// A parent→child spawn edge from the SQLite database.
#[derive(Debug, Clone)]
pub struct SpawnEdge {
    pub parent_thread_id: String,
    pub child_thread_id: String,
    pub status: String,
}

/// Snapshot classification for an observed Codex thread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodexThreadClassification {
    TopLevel,
    Child { parent_thread_id: Option<String> },
}

/// An observed Codex thread enriched with identity and classification.
#[derive(Debug, Clone)]
pub struct ObservedCodexThread {
    pub thread: CodexThread,
    pub agent_id: AgentId,
    pub pid: Option<u32>,
    pub classification: CodexThreadClassification,
}

/// Grouped Codex observation snapshot for the current polling pass.
#[derive(Debug, Clone, Default)]
pub struct CodexSnapshot {
    pub top_level_threads: Vec<ObservedCodexThread>,
    pub child_threads: Vec<ObservedCodexThread>,
}

/// Snapshot of the active top-level tool call for a Codex session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedToolCall {
    pub name: String,
    pub args_summary: Option<String>,
}

/// Snapshot of a child thread represented as a subagent under its parent session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedCodexSubagent {
    pub thread_id: String,
    pub name: String,
    pub description: Option<String>,
    pub status: PrimaryStatus,
    pub reason: Option<NormalizedReason>,
    pub started_at: Option<DateTime<Utc>>,
}

/// Event-facing snapshot for a top-level Codex session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedCodexSession {
    pub agent_id: AgentId,
    pub pid: Option<u32>,
    pub cwd: PathBuf,
    pub branch: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub status: PrimaryStatus,
    pub final_output: Option<String>,
    pub final_output_is_completion: bool,
    pub latest_tool_call: Option<ObservedToolCall>,
    pub todos: Vec<TodoItem>,
    pub subagents: Vec<ObservedCodexSubagent>,
}

/// Reconciled Codex snapshot keyed by top-level session/thread identity.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CodexEventSnapshot {
    pub sessions: Vec<ObservedCodexSession>,
}

/// Result of loading a Codex backend snapshot from sqlite.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodexDetectionResult {
    pub backend_health: CodexBackendHealth,
    pub snapshot: CodexEventSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RolloutThreadSummary {
    pub status: PrimaryStatus,
    pub final_output: Option<String>,
    pub final_output_is_completion: bool,
    pub latest_tool_call: Option<ObservedToolCall>,
    pub todos: Vec<TodoItem>,
    pub spawned_subagents: Vec<ObservedCodexSubagent>,
}
