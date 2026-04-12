use std::collections::HashMap;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Local};
use serde_json::Value;
use tracing::warn;

use crate::domain::agents::activity::{TodoItem, TodoPriority, TodoStatus};
use crate::domain::shared::time::from_rfc3339;

use super::types::{ProjectSessionInfo, SessionRegistryEntry, TranscriptEntry};

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

pub(super) fn extract_text_fragments(value: &Value, fragments: &mut Vec<String>) {
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

pub(super) fn extract_transcript_text(obj: &Value) -> Option<String> {
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
pub(super) fn is_local_command_content(content: &str) -> bool {
    content.starts_with("<local-command-") || content.starts_with("<command-name>")
}

/// Returns true if `content` is a compaction restore message.  These appear
/// after a `compact_boundary` system entry and signal that context compaction
/// has completed.  Two distinct forms exist:
///   • auto-compact: `[restore checkpointed session agent configuration after compaction]`
///   • manual `/compact`: `This session is being continued from a previous conversation…`
pub(super) fn is_compaction_restore_message(content: &str) -> bool {
    content.contains("[restore checkpointed session agent configuration after compaction]")
        || content.starts_with("This session is being continued from a previous conversation")
}

pub(super) fn last_non_assistant_entry(entries: &[TranscriptEntry]) -> Option<&TranscriptEntry> {
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

/// Returns true if `mtime` is older than `threshold_secs` seconds ago.
pub fn is_stale(mtime: std::time::SystemTime, threshold_secs: u64) -> bool {
    mtime
        .elapsed()
        .map(|e| e.as_secs() >= threshold_secs)
        .unwrap_or(false)
}

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
