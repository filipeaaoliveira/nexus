use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Result;
use serde_json::Value;
use tokio::sync::mpsc;
use tracing::{debug, warn};

use crate::event::AgentEvent;
use crate::types::PrimaryStatus;

// ── TranscriptEntry ───────────────────────────────────────────────

/// A parsed entry from a Claude transcript JSONL file.
#[derive(Debug, Clone)]
pub enum TranscriptEntry {
    User {
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
                let content = obj
                    .get("content")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                entries.push(TranscriptEntry::User { timestamp, content });
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
            _ => {
                // Ignore unknown entry types (progress, assistant, etc.)
            }
        }
    }

    entries
}

// ── infer_status_from_transcript ──────────────────────────────────

/// Infer the current status of a Claude session from its transcript entries.
///
/// - ToolUse (last) → ToolCall (tool is executing)
/// - ToolResult (last) → Running (agent is generating after receiving result)
/// - User (last) → Running (agent is processing user input)
/// - Empty → Unknown
pub fn infer_status_from_transcript(entries: &[TranscriptEntry]) -> InferredStatus {
    match entries.last() {
        Some(TranscriptEntry::ToolUse { .. }) => InferredStatus::ToolCall,
        Some(TranscriptEntry::ToolResult { .. }) => InferredStatus::Running,
        Some(TranscriptEntry::User { .. }) => InferredStatus::Running,
        None => InferredStatus::Unknown,
    }
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

// ── run_claude_detector ───────────────────────────────────────────

/// Polling loop that watches Claude transcript files for changes.
///
/// Scans `claude_data_dir/transcripts/` for `.jsonl` files, tracks their
/// sizes, and parses new content when files grow. Emits events via `tx`.
///
/// Note: This detector does not yet map sessions to PIDs — that mapping
/// is handled by the integration layer (Task 6).
pub async fn run_claude_detector(
    _tx: mpsc::Sender<AgentEvent>,
    claude_data_dir: PathBuf,
    poll_interval: Duration,
) -> Result<()> {
    let transcripts_dir = claude_data_dir.join("transcripts");

    // Track file sizes to detect new content
    let mut file_sizes: HashMap<PathBuf, u64> = HashMap::new();

    loop {
        if transcripts_dir.exists() {
            let dir = transcripts_dir.clone();
            let sizes = file_sizes.clone();
            let scan_result = tokio::task::spawn_blocking(move || {
                let mut new_sizes = sizes;
                let mut results: Vec<(PathBuf, u64, Vec<TranscriptEntry>)> = Vec::new();

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
                        let prev_size = new_sizes.get(&file_path).copied().unwrap_or(0);

                        if current_size > prev_size {
                            if let Ok(content) = std::fs::read_to_string(&file_path) {
                                let entries = parse_transcript(&content);
                                results.push((file_path.clone(), current_size, entries));
                            }
                            new_sizes.insert(file_path, current_size);
                        }
                    }
                }

                (new_sizes, results)
            })
            .await?;

            let (new_sizes, results) = scan_result;
            file_sizes = new_sizes;

            for (file_path, current_size, entries) in &results {
                let prev_size = file_sizes
                    .get(file_path)
                    .copied()
                    .unwrap_or(0)
                    .saturating_sub(*current_size);
                debug!(
                    path = %file_path.display(),
                    prev_size,
                    current_size,
                    "transcript file grew"
                );

                let _status = infer_status_from_transcript(entries);

                // Event emission will be added in Task 6
                // when PID-to-session mapping is available.
            }
        }

        tokio::time::sleep(poll_interval).await;
    }
}
