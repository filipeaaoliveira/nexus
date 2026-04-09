use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Result;
use chrono::{DateTime, Local};
use serde_json::Value;
use tokio::sync::mpsc;
use tracing::warn;

use crate::detect::registry::{AgentRegistry, ClaudeLaunchInfo};
use crate::event::AgentEvent;
use crate::types::{
    from_rfc3339, Activity, AgentId, AgentTool, NormalizedReason, NormalizedState,
    NormalizedStatus, PrimaryStatus, TodoItem, TodoPriority, TodoStatus,
};

/// Seconds since last transcript write before we consider the agent to be
/// waiting for user input. Derived from transcript gap analysis: p90 = 25.6 s.
const CLAUDE_IDLE_SECS: u64 = 10;

// ── TranscriptEntry ───────────────────────────────────────────────

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

// ── InferredStatus ────────────────────────────────────────────────

/// Status inferred from the last transcript entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InferredStatus {
    Running,
    Waiting,
    ToolCall,
    Unknown,
}

// ── ProjectSessionInfo ────────────────────────────────────────────

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

// ── parse_transcript ──────────────────────────────────────────────

/// Parse a Claude transcript JSONL string into a list of entries.
/// Each line is a JSON object with at least a "type" and "timestamp" field.
pub fn parse_transcript(content: &str) -> Vec<TranscriptEntry> {
    let mut entries = Vec::new();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let obj: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(e) => {
                warn!("failed to parse transcript line: {e}");
                continue;
            }
        };

        let entry_type = match obj.get("type").and_then(|v| v.as_str()) {
            Some(t) => t,
            None => continue,
        };

        let timestamp = obj
            .get("timestamp")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        match entry_type {
            "user" => {
                // Project-file format stores tool results inside
                // message.content as an array of {type:"tool_result",...} blocks.
                // In that case emit ToolResult entries and skip the User entry
                // (the user turn is purely carrying tool results, not a human
                // message).  The old flat-transcript format uses a top-level
                // "content" string, which falls through to the User branch below.
                let mut tool_result_emitted = false;
                if let Some(message) = obj.get("message") {
                    if let Some(arr) = message.get("content").and_then(|v| v.as_array()) {
                        for block in arr {
                            if block.get("type").and_then(|v| v.as_str()) == Some("tool_result") {
                                // The nested project-file format does not carry the
                                // tool name in the tool_result block — only tool_use_id
                                // (a correlation ID, not a human-readable name).
                                // tool_name is not used by any inference logic (only
                                // the variant matters), so leave it empty rather than
                                // storing a misleading ID.
                                entries.push(TranscriptEntry::ToolResult {
                                    timestamp: timestamp.clone(),
                                    tool_name: String::new(),
                                });
                                tool_result_emitted = true;
                            }
                        }
                    }
                }
                if !tool_result_emitted {
                    let content = obj
                        .get("content")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    if is_local_command_content(&content) {
                        entries.push(TranscriptEntry::LocalCommand { timestamp });
                    } else {
                        entries.push(TranscriptEntry::User { timestamp, content });
                    }
                }
            }
            "assistant" => {
                // Project-file format stores tool calls as {type:"tool_use",...}
                // blocks inside message.content alongside text and thinking blocks.
                // Emit each as a ToolUse entry *before* the Assistant text entry so
                // that state-inference (which scans in reverse) sees the tool call
                // as more recent than any preceding text.
                let stop_reason = obj
                    .get("message")
                    .and_then(|m| m.get("stop_reason"))
                    .and_then(|v| v.as_str())
                    .map(str::to_string);

                if let Some(message) = obj.get("message") {
                    if let Some(arr) = message.get("content").and_then(|v| v.as_array()) {
                        for block in arr {
                            if block.get("type").and_then(|v| v.as_str()) == Some("tool_use") {
                                let tool_name = block
                                    .get("name")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .to_string();
                                let tool_input = block
                                    .get("input")
                                    .and_then(|v| v.as_object())
                                    .cloned()
                                    .unwrap_or_default();
                                entries.push(TranscriptEntry::ToolUse {
                                    timestamp: timestamp.clone(),
                                    tool_name,
                                    tool_input,
                                });
                            }
                        }
                    }
                }
                let content = extract_transcript_text(&obj).unwrap_or_default();
                entries.push(TranscriptEntry::Assistant {
                    timestamp: timestamp.clone(),
                    content,
                });
                // After the assistant entry, emit TurnBlocked for stop reasons that
                // indicate the agent cannot continue without external intervention.
                if matches!(
                    stop_reason.as_deref(),
                    Some("max_tokens") | Some("refusal") | Some("pause_turn")
                ) {
                    entries.push(TranscriptEntry::TurnBlocked {
                        timestamp,
                        stop_reason: stop_reason.unwrap_or_default(),
                    });
                }
            }
            "tool_use" => {
                let tool_name = obj
                    .get("tool_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let tool_input = obj
                    .get("tool_input")
                    .and_then(|v| v.as_object())
                    .cloned()
                    .unwrap_or_default();
                entries.push(TranscriptEntry::ToolUse {
                    timestamp,
                    tool_name,
                    tool_input,
                });
            }
            "tool_result" => {
                let tool_name = obj
                    .get("tool_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                entries.push(TranscriptEntry::ToolResult {
                    timestamp,
                    tool_name,
                });
            }
            "system" => {
                let subtype = obj.get("subtype").and_then(|v| v.as_str()).unwrap_or("");
                match subtype {
                    "turn_duration" => {
                        entries.push(TranscriptEntry::TurnCompleted { timestamp });
                    }
                    "api_error" => {
                        let retry_attempt = obj
                            .get("retryAttempt")
                            .and_then(|v| v.as_u64())
                            .map(|v| v as u32);
                        let max_retries = obj
                            .get("maxRetries")
                            .and_then(|v| v.as_u64())
                            .map(|v| v as u32);
                        entries.push(TranscriptEntry::ApiError {
                            timestamp,
                            retry_attempt,
                            max_retries,
                        });
                    }
                    "compact_boundary" => {
                        entries.push(TranscriptEntry::CompactBoundary { timestamp });
                    }
                    _ => {
                        // Ignore other system subtypes (local_command, etc.)
                    }
                }
            }
            _ => {
                // Ignore unknown entry types (file-history-snapshot, last-prompt, etc.)
            }
        }
    }

    entries
}

fn extract_text_fragments(value: &Value, fragments: &mut Vec<String>) {
    match value {
        Value::String(text) => {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                fragments.push(trimmed.to_string());
            }
        }
        Value::Array(items) => {
            for item in items {
                extract_text_fragments(item, fragments);
            }
        }
        Value::Object(map) => {
            if let Some(text) = map.get("text").and_then(Value::as_str) {
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    fragments.push(trimmed.to_string());
                }
            }
            if let Some(content) = map.get("content") {
                extract_text_fragments(content, fragments);
            }
            if let Some(parts) = map.get("parts") {
                extract_text_fragments(parts, fragments);
            }
        }
        _ => {}
    }
}

fn extract_transcript_text(obj: &Value) -> Option<String> {
    let mut fragments = Vec::new();

    if let Some(content) = obj.get("content") {
        extract_text_fragments(content, &mut fragments);
    } else if let Some(message) = obj.get("message") {
        if let Some(content) = message.get("content") {
            extract_text_fragments(content, &mut fragments);
        } else if let Some(text) = message.get("text") {
            extract_text_fragments(text, &mut fragments);
        }
    } else if let Some(text) = obj.get("text") {
        extract_text_fragments(text, &mut fragments);
    }

    if fragments.is_empty() {
        None
    } else {
        Some(fragments.join("\n"))
    }
}

/// Returns true if `content` belongs to a local command injection entry that
/// Claude Code writes when a slash command (e.g. `/compact`) is executed.
/// These entries carry metadata about the command, not user intent.
fn is_local_command_content(content: &str) -> bool {
    content.starts_with("<local-command-") || content.starts_with("<command-name>")
}

/// Returns true if `content` is a compaction restore message.  These appear
/// after a `compact_boundary` system entry and signal that context compaction
/// has completed.  Two distinct forms exist:
///   • auto-compact: `[restore checkpointed session agent configuration after compaction]`
///   • manual `/compact`: `This session is being continued from a previous conversation…`
fn is_compaction_restore_message(content: &str) -> bool {
    content.contains("[restore checkpointed session agent configuration after compaction]")
        || content.starts_with("This session is being continued from a previous conversation")
}

fn last_non_assistant_entry(entries: &[TranscriptEntry]) -> Option<&TranscriptEntry> {
    // CompactBoundary and TurnBlocked are intentionally NOT excluded — they are
    // meaningful signals that last_non_assistant_entry should surface.
    // LocalCommand entries are excluded: they are metadata injected by Claude
    // Code during slash command execution and carry no user intent.
    entries.iter().rev().find(|entry| {
        !matches!(
            entry,
            TranscriptEntry::Assistant { .. }
                | TranscriptEntry::TurnCompleted { .. }
                | TranscriptEntry::ApiError { .. }
                | TranscriptEntry::LocalCommand { .. }
        )
    })
}

fn contains_review_section_marker(text: &str, marker: &str) -> bool {
    text.lines().map(str::trim_start).any(|line| {
        let line = line.trim_start_matches(['#', '>', '-', '*', ' ', '\t']);
        let lower = line.to_ascii_lowercase();

        lower == marker
            || lower.starts_with(&format!("{marker}:"))
            || lower.starts_with(&format!("{marker} -"))
            || lower.starts_with(&format!("{marker} "))
    })
}

fn looks_like_review_output(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return false;
    }

    let lower = trimmed.to_ascii_lowercase();
    if [
        "### issues",
        "#### critical",
        "merge-readiness assessment",
        "review assessment",
        "review findings",
        "audit findings",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
    {
        return true;
    }

    ["requested changes"]
        .iter()
        .any(|marker| contains_review_section_marker(trimmed, marker))
}

fn looks_like_terminal_failure_output(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return false;
    }

    let lower = trimmed.to_ascii_lowercase();
    let failure_markers = [
        "i'm unable to continue",
        "i am unable to continue",
        "i can't continue",
        "i cannot continue",
        "i'm blocked",
        "i am blocked",
        "cannot continue",
        "can't continue",
    ];

    if failure_markers.iter().any(|marker| lower.contains(marker)) {
        return true;
    }

    false
}

fn looks_like_done_output(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return false;
    }

    let lower = trimmed.to_ascii_lowercase();
    [
        "ready for the next task",
        "ready for next task",
        "ready for the next steps",
        "ready for next steps",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
}

pub fn normalize_claude_state(entries: &[TranscriptEntry], is_stale: bool) -> NormalizedState {
    let classify_assistant = |content: &str| -> Option<NormalizedState> {
        let trimmed = content.trim();
        if trimmed.is_empty() {
            return None;
        }

        if looks_like_review_output(trimmed) {
            return Some(NormalizedState::new(
                NormalizedStatus::NeedsInput,
                Some(NormalizedReason::Review),
            ));
        }

        if looks_like_terminal_failure_output(trimmed) {
            return Some(NormalizedState::new(
                NormalizedStatus::Blocked,
                Some(NormalizedReason::Error),
            ));
        }

        if looks_like_done_output(trimmed) {
            return Some(NormalizedState::new(
                NormalizedStatus::Done,
                Some(NormalizedReason::ReadyForNextTask),
            ));
        }

        None
    };

    if is_stale {
        for entry in entries.iter().rev() {
            match entry {
                TranscriptEntry::TurnCompleted { .. } => {
                    // Turn completed cleanly — agent is done and waiting for
                    // the next user message.
                    return NormalizedState::new(
                        NormalizedStatus::Done,
                        Some(NormalizedReason::ReadyForNextTask),
                    );
                }
                TranscriptEntry::ApiError { .. } => {
                    // API error with retry — agent is retrying, just slow.
                    return NormalizedState::new(
                        NormalizedStatus::Working,
                        Some(NormalizedReason::Retrying),
                    );
                }
                TranscriptEntry::TurnBlocked { .. } => {
                    // Token limit or refusal — agent is permanently blocked.
                    return NormalizedState::new(
                        NormalizedStatus::Blocked,
                        Some(NormalizedReason::Error),
                    );
                }
                TranscriptEntry::CompactBoundary { .. } => {
                    // Context compaction is underway.
                    return NormalizedState::new(
                        NormalizedStatus::Working,
                        Some(NormalizedReason::Compacting),
                    );
                }
                TranscriptEntry::ToolUse { .. } => {
                    // Stale with a pending tool call.  The most common cause
                    // is a permission prompt waiting for the user to approve.
                    // It could also mean the process exited mid-execution, but
                    // NeedsInput(Approval) is the most actionable label in
                    // either case — it surfaces the session to the user rather
                    // than silently showing Unknown.
                    return NormalizedState::new(
                        NormalizedStatus::NeedsInput,
                        Some(NormalizedReason::Approval),
                    );
                }
                TranscriptEntry::ToolResult { .. } => {
                    // Stale after a tool result: the agent was mid-turn when it
                    // went quiet (received a result, was about to generate).
                    // The session is available for input — show Done so the user
                    // knows they can send the next task.
                    return NormalizedState::new(
                        NormalizedStatus::Done,
                        Some(NormalizedReason::ReadyForNextTask),
                    );
                }
                TranscriptEntry::User { content, .. } => {
                    // A compaction restore message means the session just finished
                    // compacting and is ready for the next task — not unknown.
                    if is_compaction_restore_message(content) {
                        return NormalizedState::new(
                            NormalizedStatus::Done,
                            Some(NormalizedReason::ReadyForNextTask),
                        );
                    }
                    // Stale after a user message: the agent was responding when it
                    // went quiet (interrupted mid-turn). The session is available
                    // for input — show Done so the user knows they can continue.
                    return NormalizedState::new(
                        NormalizedStatus::Done,
                        Some(NormalizedReason::ReadyForNextTask),
                    );
                }
                TranscriptEntry::LocalCommand { .. } => {
                    // Local command metadata — transparent to state inference.
                    continue;
                }
                TranscriptEntry::Assistant { content, .. } => {
                    let trimmed = content.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    return classify_assistant(trimmed).unwrap_or_else(|| {
                        NormalizedState::new(
                            NormalizedStatus::Done,
                            Some(NormalizedReason::ReadyForNextTask),
                        )
                    });
                }
            }
        }

        // entries is empty (any ToolUse/ToolResult/User entry would have
        // returned Unknown above). Treat an empty stale transcript as Done
        // to avoid perpetually showing Unknown for sessions that were never
        // written to before becoming stale.
        return NormalizedState::new(
            NormalizedStatus::Done,
            Some(NormalizedReason::ReadyForNextTask),
        );
    }

    for entry in entries.iter().rev() {
        match entry {
            TranscriptEntry::TurnCompleted { .. } => {
                // Turn just completed — agent finished its response and is
                // waiting for the next user message.
                return NormalizedState::new(
                    NormalizedStatus::Done,
                    Some(NormalizedReason::ReadyForNextTask),
                );
            }
            TranscriptEntry::ApiError { .. } => {
                // API returned an error — agent is retrying.
                return NormalizedState::new(
                    NormalizedStatus::Working,
                    Some(NormalizedReason::Retrying),
                );
            }
            TranscriptEntry::TurnBlocked { .. } => {
                // Token limit or refusal — agent cannot continue.
                return NormalizedState::new(
                    NormalizedStatus::Blocked,
                    Some(NormalizedReason::Error),
                );
            }
            TranscriptEntry::CompactBoundary { .. } => {
                // Context compaction in progress.
                return NormalizedState::new(
                    NormalizedStatus::Working,
                    Some(NormalizedReason::Compacting),
                );
            }
            TranscriptEntry::ToolUse { tool_name, .. } => {
                // "Agent" is the exact tool name Claude Code uses for spawning
                // subagents — capitalisation matches the transcript JSON literal.
                let reason = if tool_name == "Agent" {
                    NormalizedReason::Subagent
                } else {
                    NormalizedReason::ToolCall
                };
                return NormalizedState::new(NormalizedStatus::Working, Some(reason));
            }
            TranscriptEntry::ToolResult { .. } => {
                return NormalizedState::new(
                    NormalizedStatus::Working,
                    Some(NormalizedReason::ToolCall),
                );
            }
            TranscriptEntry::User { content, .. } => {
                // A compaction restore message means compaction just finished;
                // the session is ready for the next task, not actively working.
                if is_compaction_restore_message(content) {
                    return NormalizedState::new(
                        NormalizedStatus::Done,
                        Some(NormalizedReason::ReadyForNextTask),
                    );
                }
                return NormalizedState::new(NormalizedStatus::Working, None);
            }
            TranscriptEntry::LocalCommand { .. } => {
                // Local command metadata — transparent to state inference.
                continue;
            }
            TranscriptEntry::Assistant { content, .. } => {
                let trimmed = content.trim();
                if trimmed.is_empty() {
                    continue;
                }
                return classify_assistant(trimmed)
                    .unwrap_or_else(|| NormalizedState::new(NormalizedStatus::Working, None));
            }
        }
    }

    NormalizedState::new(NormalizedStatus::Unknown, None)
}

// ── infer_status_from_transcript ──────────────────────────────────

/// Infer the current status of a Claude session from its transcript entries.
///
/// - ToolUse (last) → ToolCall (tool is executing)
/// - ToolResult (last) → Running (agent is generating after receiving result)
/// - User (last) → Running (agent is processing user input)
/// - Empty → Unknown
pub fn infer_status_from_transcript(entries: &[TranscriptEntry]) -> InferredStatus {
    match last_non_assistant_entry(entries) {
        Some(TranscriptEntry::ToolUse { .. }) => InferredStatus::ToolCall,
        Some(TranscriptEntry::ToolResult { .. }) => InferredStatus::Running,
        Some(TranscriptEntry::User { .. }) => InferredStatus::Running,
        Some(TranscriptEntry::CompactBoundary { .. }) => InferredStatus::Running,
        Some(TranscriptEntry::TurnBlocked { .. }) => InferredStatus::Unknown,
        // LocalCommand is excluded by last_non_assistant_entry, so this arm is
        // unreachable in practice but required for exhaustive matching.
        Some(TranscriptEntry::LocalCommand { .. }) => InferredStatus::Unknown,
        Some(TranscriptEntry::Assistant { .. })
        | Some(TranscriptEntry::TurnCompleted { .. })
        | Some(TranscriptEntry::ApiError { .. })
        | None => InferredStatus::Unknown,
    }
}

// ── infer_activity_from_last_entry ────────────────────────────────

/// Infer the current `Activity` from the last transcript entry.
///
/// - ToolUse (last) → Activity::ToolCall  (a tool is executing right now)
/// - ToolResult (last) → Activity::Thinking (agent is generating after result)
/// - User (last) → Activity::Generating   (agent is responding to user input)
/// - Empty → None
pub fn infer_activity_from_last_entry(entries: &[TranscriptEntry]) -> Option<Activity> {
    match last_non_assistant_entry(entries) {
        Some(TranscriptEntry::ToolUse { .. }) => Some(Activity::ToolCall),
        Some(TranscriptEntry::ToolResult { .. }) => Some(Activity::Thinking),
        Some(TranscriptEntry::CompactBoundary { .. }) => Some(Activity::Compacting),
        Some(TranscriptEntry::TurnBlocked { .. }) => None,
        Some(TranscriptEntry::User { content, .. }) => {
            if is_compaction_restore_message(content) {
                // Restoration message that follows a compact_boundary — still compacting.
                Some(Activity::Compacting)
            } else {
                Some(Activity::Generating)
            }
        }
        // LocalCommand is excluded by last_non_assistant_entry, so this arm is
        // unreachable in practice but required for exhaustive matching.
        Some(TranscriptEntry::LocalCommand { .. }) => None,
        Some(TranscriptEntry::Assistant { .. })
        | Some(TranscriptEntry::TurnCompleted { .. })
        | Some(TranscriptEntry::ApiError { .. })
        | None => None,
    }
}

// ── extract_todos_from_transcript ─────────────────────────────────

/// Extract the current todo list from transcript entries.
///
/// Finds the last `todowrite` tool_use call and parses its `tool_input.todos`
/// array. Returns empty vec if no todowrite calls found.
pub fn extract_todos_from_transcript(entries: &[TranscriptEntry]) -> Vec<TodoItem> {
    // Iterate in reverse to find the last todowrite call
    for entry in entries.iter().rev() {
        if let TranscriptEntry::ToolUse {
            tool_name,
            tool_input,
            ..
        } = entry
        {
            if tool_name == "todowrite" {
                if let Some(todos_value) = tool_input.get("todos") {
                    if let Some(todos_array) = todos_value.as_array() {
                        return todos_array
                            .iter()
                            .filter_map(|item| {
                                let content = item.get("content")?.as_str()?.to_string();
                                let status = item
                                    .get("status")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("pending");
                                let priority = item
                                    .get("priority")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("medium");
                                Some(TodoItem {
                                    content,
                                    status: TodoStatus::parse_str(status),
                                    priority: TodoPriority::parse_str(priority),
                                })
                            })
                            .collect();
                    }
                }
                return Vec::new();
            }
        }
    }
    Vec::new()
}

// ── is_stale ──────────────────────────────────────────────────────

/// Returns true if `mtime` is older than `threshold_secs` seconds ago.
pub fn is_stale(mtime: std::time::SystemTime, threshold_secs: u64) -> bool {
    mtime
        .elapsed()
        .map(|e| e.as_secs() >= threshold_secs)
        .unwrap_or(false)
}

// ── parse_project_session ─────────────────────────────────────────

/// Parse a Claude project session JSONL file to extract session metadata.
///
/// Extracts session ID, git branch, working directory, model, and detects
/// internal initiator markers (subagent sessions).
pub fn parse_project_session(content: &str) -> ProjectSessionInfo {
    let mut info = ProjectSessionInfo::default();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let obj: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };

        // Capture the timestamp of the first entry as session start time
        if info.started_at.is_none() {
            if let Some(ts) = obj.get("timestamp").and_then(|v| v.as_str()) {
                info.started_at = Some(ts.to_string());
            }
        }

        // Extract sessionId from any entry that has it
        if info.session_id.is_none() {
            if let Some(sid) = obj.get("sessionId").and_then(|v| v.as_str()) {
                info.session_id = Some(sid.to_string());
            }
        }

        let entry_type = match obj.get("type").and_then(|v| v.as_str()) {
            Some(t) => t,
            None => continue,
        };

        match entry_type {
            "user" => {
                // Extract branch and cwd from user entries
                if let Some(branch) = obj.get("gitBranch").and_then(|v| v.as_str()) {
                    info.branch = Some(branch.to_string());
                }
                if let Some(cwd) = obj.get("cwd").and_then(|v| v.as_str()) {
                    info.cwd = Some(PathBuf::from(cwd));
                }

                // Check for internal initiator marker in message content
                if let Some(message) = obj.get("message") {
                    if let Some(content) = message.get("content").and_then(|v| v.as_str()) {
                        if content.contains("<!-- OMO_INTERNAL_INITIATOR -->") {
                            info.has_internal_initiator = true;
                        }
                    }
                }
            }
            "assistant" => {
                // Extract model from assistant entries
                if let Some(message) = obj.get("message") {
                    if let Some(model) = message.get("model").and_then(|v| v.as_str()) {
                        info.model = Some(model.to_string());
                    }
                }
            }
            _ => {}
        }
    }

    info
}

fn transcript_entry_timestamp(entry: &TranscriptEntry) -> Option<DateTime<Local>> {
    match entry {
        TranscriptEntry::User { timestamp, .. }
        | TranscriptEntry::Assistant { timestamp, .. }
        | TranscriptEntry::ToolUse { timestamp, .. }
        | TranscriptEntry::ToolResult { timestamp, .. }
        | TranscriptEntry::TurnCompleted { timestamp, .. }
        | TranscriptEntry::ApiError { timestamp, .. }
        | TranscriptEntry::CompactBoundary { timestamp, .. }
        | TranscriptEntry::TurnBlocked { timestamp, .. }
        | TranscriptEntry::LocalCommand { timestamp, .. } => from_rfc3339(timestamp),
    }
}

/// For fresh launches, skip transcript entries older than the process start.
///
/// Resumed launches keep their full transcript history.
pub fn transcript_start_index_for_process(
    entries: &[TranscriptEntry],
    started_at: Option<DateTime<Local>>,
    is_resume_launch: bool,
) -> usize {
    if is_resume_launch {
        return 0;
    }

    let Some(started_at) = started_at else {
        return 0;
    };

    entries
        .iter()
        .position(|entry| {
            transcript_entry_timestamp(entry)
                .map(|timestamp| timestamp >= started_at)
                .unwrap_or(true)
        })
        .unwrap_or(entries.len())
}

pub fn is_resume_launch(launch: Option<&ClaudeLaunchInfo>) -> bool {
    launch
        .and_then(|info| info.resume_session_id.as_ref())
        .is_some()
}

pub fn should_hydrate_session_history(
    launch: Option<&ClaudeLaunchInfo>,
    session_info: Option<&ProjectSessionInfo>,
) -> bool {
    let Some(resume_session_id) = launch.and_then(|info| info.resume_session_id.as_deref()) else {
        return false;
    };

    session_info
        .and_then(|info| info.session_id.as_deref())
        .is_some_and(|session_id| session_id == resume_session_id)
}

/// Decide whether the detector can permanently mark SessionInfo as handled.
///
/// Fresh launches can mark it handled immediately even if no historical session
/// data is available. Resume launches must wait until the matching historical
/// session is actually emitted.
pub fn should_mark_session_info_sent(
    is_resume_launch: bool,
    historical_session_emitted: bool,
) -> bool {
    !is_resume_launch || historical_session_emitted
}

// ── inferred_to_primary ───────────────────────────────────────────

/// Map an inferred transcript status to a primary display status.
pub fn inferred_to_primary(inferred: InferredStatus) -> PrimaryStatus {
    match inferred {
        InferredStatus::Running => PrimaryStatus::Running,
        InferredStatus::Waiting => PrimaryStatus::Waiting,
        InferredStatus::ToolCall => PrimaryStatus::Running,
        InferredStatus::Unknown => PrimaryStatus::Running,
    }
}

// ── find_transcript_for_pid ───────────────────────────────────────

/// Find the most recently modified transcript file matching a session prefix.
///
/// Searches `transcripts_dir` for `.jsonl` files whose name starts with
/// `session_prefix` and returns the one with the latest modification time.
pub fn find_transcript_for_pid(transcripts_dir: &Path, session_prefix: &str) -> Option<PathBuf> {
    let read_dir = std::fs::read_dir(transcripts_dir).ok()?;

    let mut best: Option<(PathBuf, std::time::SystemTime)> = None;

    for entry in read_dir.flatten() {
        let path = entry.path();
        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };

        if !name.starts_with(session_prefix) || !name.ends_with(".jsonl") {
            continue;
        }

        let modified = match entry.metadata().and_then(|m| m.modified()) {
            Ok(t) => t,
            Err(_) => continue,
        };

        if best.as_ref().is_none_or(|(_, prev)| modified > *prev) {
            best = Some((path, modified));
        }
    }

    best.map(|(p, _)| p)
}

// ── State tracking ────────────────────────────────────────────────

/// Per-session emit state to avoid duplicate events.
#[derive(Debug, Clone, Default)]
struct SessionEmitState {
    last_status: Option<PrimaryStatus>,
    last_normalized_state: Option<NormalizedState>,
    last_tool_call: Option<String>,
    emitted_subagent_tools: Vec<String>,
    session_info_sent: bool,
    entry_count: usize,
    last_mtime: Option<std::time::SystemTime>,
    pid: Option<u32>,
    last_todos: Vec<TodoItem>,
    last_entries: Vec<TranscriptEntry>,
}

// ── Session registry ──────────────────────────────────────────────

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

/// Scan `~/.claude/sessions/` and return all live session entries keyed by
/// session_id.  Files that cannot be parsed are silently skipped.
pub fn scan_session_registry(sessions_dir: &Path) -> HashMap<String, SessionRegistryEntry> {
    let mut map = HashMap::new();
    let Ok(read_dir) = std::fs::read_dir(sessions_dir) else {
        return map;
    };
    for entry in read_dir.flatten() {
        let path = entry.path();
        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };
        // Files are named "<pid>.json"
        if !name.ends_with(".json") {
            continue;
        }
        let pid_str = name.trim_end_matches(".json");
        let pid: u32 = match pid_str.parse() {
            Ok(p) => p,
            Err(_) => continue,
        };
        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let obj: serde_json::Value = match serde_json::from_str(&content) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let session_id = match obj.get("sessionId").and_then(|v| v.as_str()) {
            Some(s) => s.to_string(),
            None => continue,
        };
        let cwd = match obj.get("cwd").and_then(|v| v.as_str()) {
            Some(c) => PathBuf::from(c),
            None => continue,
        };
        let kind = obj
            .get("kind")
            .and_then(|v| v.as_str())
            .unwrap_or("interactive")
            .to_string();
        map.insert(
            session_id.clone(),
            SessionRegistryEntry {
                pid,
                session_id,
                cwd,
                kind,
            },
        );
    }
    map
}

// ── run_claude_detector ───────────────────────────────────────────

/// Polling loop that watches Claude transcript and project session files.
///
/// Scans `claude_data_dir/projects/` for project session JSONL files to
/// extract CWD→session mappings, then reads transcript files to emit
/// enrichment events (status, tool calls, subagents).
pub async fn run_claude_detector(
    tx: mpsc::Sender<AgentEvent>,
    claude_data_dir: PathBuf,
    poll_interval: Duration,
    registry: AgentRegistry,
) -> Result<()> {
    let transcripts_dir = claude_data_dir.join("transcripts");
    let projects_dir = claude_data_dir.join("projects");
    let sessions_dir = claude_data_dir.join("sessions");

    // Combined per-file state: size and mtime tracked together to avoid two
    // separate HashMap lookups/inserts for every transcript file each poll.
    #[derive(Clone, Copy)]
    struct TranscriptFileState {
        size: u64,
        mtime: std::time::SystemTime,
    }
    impl Default for TranscriptFileState {
        fn default() -> Self {
            Self {
                size: 0,
                mtime: std::time::SystemTime::UNIX_EPOCH,
            }
        }
    }
    let mut file_state: HashMap<PathBuf, TranscriptFileState> = HashMap::new();
    // Track emit state per session (keyed by session_id or transcript path)
    let mut session_states: HashMap<String, SessionEmitState> = HashMap::new();

    // Phase-1 cache: project session files keyed by file path → (mtime, parsed info).
    // The file is only re-read when its mtime advances.
    let mut project_file_cache: HashMap<PathBuf, (std::time::SystemTime, ProjectSessionInfo)> =
        HashMap::new();

    loop {
        // Phase 0: Scan ~/.claude/sessions/<pid>.json to build session_id → pid map.
        // These files are written by Claude Code while a process is live.
        let sdir = sessions_dir.clone();
        let session_registry: HashMap<String, SessionRegistryEntry> =
            tokio::task::spawn_blocking(move || scan_session_registry(&sdir)).await?;
        // Build a session_id → pid lookup from the live registry.
        let session_id_to_pid: HashMap<String, u32> = session_registry
            .values()
            .map(|e| (e.session_id.clone(), e.pid))
            .collect();

        // Phase 1: Scan project session files to build CWD→session_id map.
        // Re-reads a file only when its mtime has changed since the last poll.
        let mut cwd_to_session: HashMap<PathBuf, ProjectSessionInfo> = HashMap::new();

        if projects_dir.exists() {
            let pdir = projects_dir.clone();
            // Snapshot only the mtimes we already know; move the snapshot (not
            // the cache) into spawn_blocking so we can compare without cloning
            // the full ProjectSessionInfo values.
            let known_mtimes: HashMap<PathBuf, std::time::SystemTime> = project_file_cache
                .iter()
                .map(|(p, (m, _))| (p.clone(), *m))
                .collect();

            let project_scan = tokio::task::spawn_blocking(move || {
                let mut changed: Vec<(PathBuf, std::time::SystemTime, String)> = Vec::new();
                let mut unchanged: Vec<PathBuf> = Vec::new();

                if let Ok(project_dirs) = std::fs::read_dir(&pdir) {
                    for project_entry in project_dirs.flatten() {
                        let project_path = project_entry.path();
                        if !project_path.is_dir() {
                            continue;
                        }
                        if let Ok(session_files) = std::fs::read_dir(&project_path) {
                            for file_entry in session_files.flatten() {
                                let file_path = file_entry.path();
                                let name = match file_path.file_name().and_then(|n| n.to_str()) {
                                    Some(n) => n.to_string(),
                                    None => continue,
                                };
                                if !name.ends_with(".jsonl") {
                                    continue;
                                }
                                let mtime = file_entry
                                    .metadata()
                                    .ok()
                                    .and_then(|m| m.modified().ok())
                                    .unwrap_or(std::time::SystemTime::UNIX_EPOCH);

                                if known_mtimes.get(&file_path).copied() == Some(mtime) {
                                    // Mtime unchanged — use cached value.
                                    unchanged.push(file_path);
                                } else if let Ok(content) = std::fs::read_to_string(&file_path) {
                                    changed.push((file_path, mtime, content));
                                }
                            }
                        }
                    }
                }
                (changed, unchanged)
            })
            .await?;

            let (changed, unchanged) = project_scan;

            // Update the cache for changed files.
            for (path, mtime, content) in changed {
                let info = parse_project_session(&content);
                if info.cwd.is_some() {
                    project_file_cache.insert(path, (mtime, info));
                }
            }
            // Populate cwd_to_session from cache (both changed and unchanged).
            for path in &unchanged {
                if let Some((_, info)) = project_file_cache.get(path) {
                    if let Some(ref cwd) = info.cwd {
                        cwd_to_session.insert(cwd.clone(), info.clone());
                    }
                }
            }
            // Also include the newly-read changed entries.
            for (_, info) in project_file_cache.values() {
                if let Some(ref cwd) = info.cwd {
                    cwd_to_session
                        .entry(cwd.clone())
                        .or_insert_with(|| info.clone());
                }
            }
        }

        // Phase 2: Scan transcripts AND project session files for conversation
        // content and emit enrichment events.
        //
        // There are two distinct Claude JSONL formats:
        //
        //  • ~/.claude/transcripts/ses_*.jsonl  — flat format written by
        //    subagent (child) Claude Code processes.  Has top-level
        //    "tool_use" / "tool_result" entries but no "assistant" entries.
        //
        //  • ~/.claude/projects/<proj>/<uuid>.jsonl  — nested API format
        //    written by the main (parent) Claude Code process.  Has
        //    "assistant" entries with tool_use blocks inside message.content,
        //    and "user" entries with tool_result blocks inside message.content.
        //
        // For the main session we must read from projects/ so that assistant
        // text is available for Done/NeedsInput state classification.
        {
            // Snapshot (path, cwd) for every project session file Phase 1 cached,
            // deduplicated by CWD keeping only the most recently modified file per CWD.
            // This prevents multiple old session files for the same project from each
            // firing the staleness check and emitting duplicate status-change events.
            let project_file_cwds: Vec<(PathBuf, PathBuf)> = {
                let mut by_cwd: HashMap<PathBuf, (PathBuf, std::time::SystemTime)> =
                    HashMap::new();
                for (path, (mtime, info)) in &project_file_cache {
                    if let Some(cwd) = &info.cwd {
                        let entry = by_cwd
                            .entry(cwd.clone())
                            .or_insert_with(|| (path.clone(), *mtime));
                        if mtime >= &entry.1 {
                            *entry = (path.clone(), *mtime);
                        }
                    }
                }
                by_cwd
                    .into_iter()
                    .map(|(cwd, (path, _))| (path, cwd))
                    .collect()
            };

            let dir = transcripts_dir.clone();
            // Move the combined file-state map into spawn_blocking; this is an
            // O(1) pointer move, not a clone.
            let prev_state = std::mem::take(&mut file_state);
            // Each result is (file_path, entries, cwd_hint).
            // cwd_hint is Some(cwd) for project session files (use CWD-based PID
            // lookup) and None for transcript files (use session-id matching).
            let scan_result = tokio::task::spawn_blocking(move || {
                let mut new_state = prev_state;
                let mut results: Vec<(PathBuf, Vec<TranscriptEntry>, Option<PathBuf>)> =
                    Vec::new();

                if let Ok(read_dir) = std::fs::read_dir(&dir) {
                    for entry in read_dir.flatten() {
                        let file_path = entry.path();
                        let name = match file_path.file_name().and_then(|n| n.to_str()) {
                            Some(n) => n.to_string(),
                            None => continue,
                        };

                        if !name.ends_with(".jsonl") {
                            continue;
                        }

                        let metadata = match entry.metadata() {
                            Ok(m) => m,
                            Err(_) => continue,
                        };

                        let current_size = metadata.len();
                        let mtime = metadata
                            .modified()
                            .unwrap_or(std::time::SystemTime::UNIX_EPOCH);

                        let entry_state = new_state.entry(file_path.clone()).or_default();
                        let prev_size = entry_state.size;

                        // Always update the mtime (needed for staleness detection).
                        entry_state.mtime = mtime;

                        if current_size > prev_size {
                            if let Ok(content) = std::fs::read_to_string(&file_path) {
                                let entries = parse_transcript(&content);
                                results.push((file_path.clone(), entries, None));
                            }
                            entry_state.size = current_size;
                        }
                    }
                }

                // Also scan project session files for main-session conversation
                // data (assistant text, nested tool_use/tool_result).
                for (project_path, cwd) in &project_file_cwds {
                    let metadata = match std::fs::metadata(project_path) {
                        Ok(m) => m,
                        Err(_) => continue,
                    };
                    let current_size = metadata.len();
                    let mtime = metadata
                        .modified()
                        .unwrap_or(std::time::SystemTime::UNIX_EPOCH);

                    let entry_state = new_state.entry(project_path.clone()).or_default();
                    let prev_size = entry_state.size;
                    entry_state.mtime = mtime;

                    if current_size > prev_size {
                        if let Ok(content) = std::fs::read_to_string(project_path) {
                            let entries = parse_transcript(&content);
                            results.push((project_path.clone(), entries, Some(cwd.clone())));
                        }
                        entry_state.size = current_size;
                    }
                }

                (new_state, results)
            })
            .await?;

            let (new_state, results) = scan_result;
            file_state = new_state;

            // PIDs already claimed by a project session file.  Transcript files
            // that resolve to the same PID are skipped to avoid duplicate events:
            // the project file is the authoritative source for main-session state.
            let project_file_pids: std::collections::HashSet<u32> = results
                .iter()
                .filter_map(|(_, _, cwd_hint)| {
                    cwd_hint
                        .as_ref()
                        .and_then(|cwd| registry.find_pid(AgentTool::Claude, cwd))
                })
                .collect();

            for (file_path, entries, cwd_hint) in &results {
                // Determine which Claude process owns this file.
                //
                // Project session files (cwd_hint = Some(cwd)) are matched
                // directly by CWD — no filename heuristics needed.
                //
                // Transcript files (cwd_hint = None) use the existing session-id
                // filename matching with a single-process fallback, but are
                // skipped when a project file already covers that PID to prevent
                // duplicate events for the same logical session.
                let mut matched_pid: Option<u32> = None;
                let mut matched_session: Option<&ProjectSessionInfo> = None;

                if let Some(cwd) = cwd_hint {
                    // For project session files, first try to match via
                    // session_id in the live session registry (most reliable
                    // when Claude wrote the file cleanly).  Fall back to
                    // CWD-based lookup from the process registry.
                    let session_info = cwd_to_session.get(cwd);
                    let registry_pid = session_info
                        .and_then(|si| si.session_id.as_deref())
                        .and_then(|sid| session_id_to_pid.get(sid).copied());

                    if let Some(pid) = registry_pid.or_else(|| registry.find_pid(AgentTool::Claude, cwd)) {
                        matched_pid = Some(pid);
                        matched_session = session_info;
                    }
                } else {
                    // Transcript file: match by session_id in filename, with
                    // single-process fallback.
                    for (cwd, session_info) in &cwd_to_session {
                        if let Some(pid) = registry.find_pid(AgentTool::Claude, cwd) {
                            if let Some(ref sid) = session_info.session_id {
                                if let Some(fname) =
                                    file_path.file_name().and_then(|n| n.to_str())
                                {
                                    if fname.contains(sid) {
                                        matched_pid = Some(pid);
                                        matched_session = Some(session_info);
                                        break;
                                    }
                                }
                            }
                            if matched_pid.is_none() {
                                matched_pid = Some(pid);
                                matched_session = Some(session_info);
                            }
                        }
                    }

                    // If no project session match, try all known Claude PIDs
                    if matched_pid.is_none() {
                        let claude_agents = registry.agents_for_tool(AgentTool::Claude);
                        if claude_agents.len() == 1 {
                            matched_pid = Some(claude_agents[0].1);
                        }
                    }

                    // Skip this transcript file if a project session file already
                    // handles the same PID — the project file is authoritative for
                    // main-session conversation state.
                    if matched_pid.is_some_and(|pid| project_file_pids.contains(&pid)) {
                        continue;
                    }
                }

                let pid = match matched_pid {
                    Some(p) => p,
                    None => continue, // no matching process discovered yet
                };
                let launch_info = registry.find_claude_launch(pid);
                let resume_launch = is_resume_launch(launch_info.as_ref());
                let hydrate_session_history =
                    should_hydrate_session_history(launch_info.as_ref(), matched_session);
                let process_started_at = launch_info.as_ref().and_then(|info| info.started_at);

                let session_key = file_path.to_string_lossy().to_string();
                let state = session_states.entry(session_key).or_default();

                state.pid = Some(pid);

                // Emit SessionInfo once
                if !state.session_info_sent {
                    let mut historical_session_emitted = false;
                    if hydrate_session_history {
                        if let Some(session_info) = matched_session {
                            let session_started =
                                session_info.started_at.as_deref().and_then(from_rfc3339);
                            let _ = tx
                                .send(AgentEvent::SessionInfo {
                                    id: AgentId::new(AgentTool::Claude, pid),
                                    pid: Some(pid),
                                    session_id: session_info.session_id.clone().unwrap_or_default(),
                                    branch: session_info.branch.clone(),
                                    is_worktree: false,
                                    started_at: session_started,
                                })
                                .await;
                            historical_session_emitted = true;
                        }
                    }
                    state.session_info_sent =
                        should_mark_session_info_sent(resume_launch, historical_session_emitted);
                }

                let transcript_start_index =
                    transcript_start_index_for_process(entries, process_started_at, resume_launch);
                if state.entry_count < transcript_start_index {
                    state.entry_count = transcript_start_index;
                }

                // Only process new entries since last check
                if entries.len() <= state.entry_count {
                    continue;
                }

                let new_entries = &entries[state.entry_count..];
                state.entry_count = entries.len();
                state.last_mtime = file_state.get(file_path).map(|s| s.mtime);

                // If we were Waiting and new entries arrived, restore Running
                if state.last_status == Some(PrimaryStatus::Waiting) {
                    let _ = tx
                        .send(AgentEvent::StatusChanged {
                            id: AgentId::new(AgentTool::Claude, pid),
                            pid: Some(pid),
                            old: PrimaryStatus::Waiting,
                            new: PrimaryStatus::Running,
                        })
                        .await;
                    state.last_status = Some(PrimaryStatus::Running);
                }

                let relevant_entries = &entries[transcript_start_index..];

                // Infer status from the processed transcript slice.
                let inferred = infer_status_from_transcript(relevant_entries);
                let primary = inferred_to_primary(inferred);

                if state.last_status != Some(primary) {
                    let old = state.last_status.unwrap_or(PrimaryStatus::Running);
                    let _ = tx
                        .send(AgentEvent::StatusChanged {
                            id: AgentId::new(AgentTool::Claude, pid),
                            pid: Some(pid),
                            old,
                            new: primary,
                        })
                        .await;
                    state.last_status = Some(primary);
                }

                let normalized_state = normalize_claude_state(relevant_entries, false);
                if state.last_normalized_state.as_ref() != Some(&normalized_state) {
                    let _ = tx
                        .send(AgentEvent::NormalizedStateChanged {
                            id: AgentId::new(AgentTool::Claude, pid),
                            pid: Some(pid),
                            state: normalized_state.clone(),
                        })
                        .await;
                    state.last_normalized_state = Some(normalized_state);
                }

                // Emit tool call events from new entries
                for entry in new_entries {
                    match entry {
                        TranscriptEntry::ToolUse {
                            timestamp: ref entry_timestamp,
                            tool_name,
                            tool_input,
                        } => {
                            // Check for subagent spawn (Task tool with subagent_type)
                            if tool_name == "task" {
                                if let Some(subagent_type) =
                                    tool_input.get("subagent_type").and_then(|v| v.as_str())
                                {
                                    let key = format!("{}:{}", tool_name, subagent_type);
                                    if !state.emitted_subagent_tools.contains(&key) {
                                        let description = tool_input
                                            .get("description")
                                            .and_then(|v| v.as_str())
                                            .map(String::from);
                                        let _ = tx
                                            .send(AgentEvent::SubagentSpawned {
                                                id: AgentId::new(AgentTool::Claude, pid),
                                                pid: Some(pid),
                                                subagent: crate::types::Subagent {
                                                    name: subagent_type.to_string(),
                                                    status: PrimaryStatus::Running,
                                                    reason: None,
                                                    description,
                                                    started_at: from_rfc3339(entry_timestamp)
                                                        .unwrap_or_else(chrono::Local::now),
                                                    ended_at: None,
                                                },
                                            })
                                            .await;
                                        state.emitted_subagent_tools.push(key);
                                    }
                                    continue;
                                }
                            }

                            // Regular tool call
                            if state.last_tool_call.as_deref() != Some(tool_name) {
                                let args_summary = tool_input
                                    .get("command")
                                    .or_else(|| tool_input.get("filePath"))
                                    .or_else(|| tool_input.get("pattern"))
                                    .and_then(|v| v.as_str())
                                    .map(String::from);
                                let _ = tx
                                    .send(AgentEvent::ToolCallStarted {
                                        id: AgentId::new(AgentTool::Claude, pid),
                                        pid: Some(pid),
                                        tool_name: tool_name.clone(),
                                        args_summary,
                                    })
                                    .await;
                                state.last_tool_call = Some(tool_name.clone());
                            }
                        }
                        TranscriptEntry::ToolResult { .. } => {
                            // ToolResult means the agent received the result and is about to think.
                            // Do NOT emit ToolCallCompleted here — that would set activity=Idle.
                            // Activity will be updated via ActivityChanged in Task 3.
                        }
                        _ => {}
                    }
                }

                // Emit ActivityChanged based on the retained slice so fresh-launch
                // cutoff behavior stays consistent with status, normalized state,
                // and todos.
                let new_activity = infer_activity_from_last_entry(relevant_entries);
                let _ = tx
                    .send(AgentEvent::ActivityChanged {
                        id: AgentId::new(AgentTool::Claude, pid),
                        pid: Some(pid),
                        activity: new_activity,
                    })
                    .await;

                // Extract todos and emit if changed
                let todos = extract_todos_from_transcript(relevant_entries);
                if todos != state.last_todos {
                    let _ = tx
                        .send(AgentEvent::TodosUpdated {
                            id: AgentId::new(AgentTool::Claude, pid),
                            pid: Some(pid),
                            todos: todos.clone(),
                        })
                        .await;
                    state.last_todos = todos;
                }

                state.last_entries = relevant_entries.to_vec();
            }

            // Staleness check: if any Running session's transcript file hasn't been
            // written for CLAUDE_IDLE_SECS, emit StatusChanged → Waiting.
            for (session_key, state) in session_states.iter_mut() {
                if state.last_status != Some(PrimaryStatus::Running) {
                    continue;
                }
                let path = PathBuf::from(session_key.as_str());
                let mtime = match file_state.get(&path).map(|s| s.mtime) {
                    Some(m) => m,
                    None => continue,
                };
                if !is_stale(mtime, CLAUDE_IDLE_SECS) {
                    continue;
                }
                let pid = match state.pid {
                    Some(p) => p,
                    None => continue,
                };
                let old = state.last_status.unwrap_or(PrimaryStatus::Running);
                let _ = tx
                    .send(AgentEvent::StatusChanged {
                        id: AgentId::new(AgentTool::Claude, pid),
                        pid: Some(pid),
                        old,
                        new: PrimaryStatus::Waiting,
                    })
                    .await;
                let _ = tx
                    .send(AgentEvent::ActivityChanged {
                        id: AgentId::new(AgentTool::Claude, pid),
                        pid: Some(pid),
                        activity: None, // clear activity when Waiting
                    })
                    .await;

                let normalized_state = normalize_claude_state(&state.last_entries, true);
                if state.last_normalized_state.as_ref() != Some(&normalized_state) {
                    let _ = tx
                        .send(AgentEvent::NormalizedStateChanged {
                            id: AgentId::new(AgentTool::Claude, pid),
                            pid: Some(pid),
                            state: normalized_state.clone(),
                        })
                        .await;
                    state.last_normalized_state = Some(normalized_state);
                }

                state.last_status = Some(PrimaryStatus::Waiting);
            }
        }

        tokio::time::sleep(poll_interval).await;
    }
}
