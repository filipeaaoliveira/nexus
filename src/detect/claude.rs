use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Result;
use serde_json::Value;
use tokio::sync::mpsc;
use tracing::warn;

use crate::detect::registry::AgentRegistry;
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
                let content = obj
                    .get("content")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                entries.push(TranscriptEntry::User { timestamp, content });
            }
            "assistant" => {
                let content = extract_transcript_text(&obj).unwrap_or_default();
                entries.push(TranscriptEntry::Assistant { timestamp, content });
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
                // Ignore unknown entry types (progress, metadata, etc.)
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

fn last_non_assistant_entry(entries: &[TranscriptEntry]) -> Option<&TranscriptEntry> {
    entries
        .iter()
        .rev()
        .find(|entry| !matches!(entry, TranscriptEntry::Assistant { .. }))
}

fn contains_review_section_marker(text: &str, marker: &str) -> bool {
    text.lines().map(str::trim_start).any(|line| {
        let line =
            line.trim_start_matches(|ch: char| matches!(ch, '#' | '>' | '-' | '*' | ' ' | '\t'));
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
    ]
    .iter()
    .any(|marker| lower.contains(marker))
    {
        return true;
    }

    [
        "review findings",
        "audit findings",
        "findings",
        "review report",
        "report",
        "requested changes",
    ]
    .iter()
    .any(|marker| contains_review_section_marker(trimmed, marker))
}

fn looks_like_user_choice_frame(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    [
        "would you like me to",
        "do you want me to",
        "should i",
        "can i",
        "let me know if you'd like",
        "let me know if you want",
        "choose one",
        "pick one",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
}

fn looks_like_terminal_failure_output(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return false;
    }

    let lower = trimmed.to_ascii_lowercase();
    let failure_markers = [
        "cannot continue",
        "can't continue",
        "unable to continue",
        "unrecoverable error",
        "fatal error",
        "failed permanently",
        "blocking error",
        "i'm blocked",
        "i am blocked",
        "could not complete",
        "can't complete",
        "cannot complete",
    ];

    failure_markers.iter().any(|marker| lower.contains(marker)) && !looks_like_user_choice_frame(trimmed)
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
                TranscriptEntry::ToolUse { .. }
                | TranscriptEntry::ToolResult { .. }
                | TranscriptEntry::User { .. } => {
                    return NormalizedState::new(NormalizedStatus::Unknown, None);
                }
                TranscriptEntry::Assistant { content, .. } => {
                    let trimmed = content.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    return classify_assistant(trimmed).unwrap_or_else(|| {
                        NormalizedState::new(NormalizedStatus::Unknown, None)
                    });
                }
            }
        }

        return NormalizedState::new(NormalizedStatus::Unknown, None);
    }

    for entry in entries.iter().rev() {
        match entry {
            TranscriptEntry::ToolUse { .. } => {
                return NormalizedState::new(
                    NormalizedStatus::Working,
                    Some(NormalizedReason::ToolCall),
                );
            }
            TranscriptEntry::ToolResult { .. } | TranscriptEntry::User { .. } => {
                return NormalizedState::new(NormalizedStatus::Working, None);
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
        Some(TranscriptEntry::Assistant { .. }) | None => InferredStatus::Unknown,
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
        Some(TranscriptEntry::User { content, .. }) => {
            if content
                .contains("[restore checkpointed session agent configuration after compaction]")
            {
                Some(Activity::Compacting)
            } else {
                Some(Activity::Generating)
            }
        }
        Some(TranscriptEntry::Assistant { .. }) | None => None,
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
                                } else if let Ok(content) =
                                    std::fs::read_to_string(&file_path)
                                {
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
                    cwd_to_session.entry(cwd.clone()).or_insert_with(|| info.clone());
                }
            }
        }

        // Phase 2: Scan transcripts and emit enrichment events
        if transcripts_dir.exists() {
            let dir = transcripts_dir.clone();
            // Move the combined file-state map into spawn_blocking; this is an
            // O(1) pointer move, not a clone.
            let prev_state = std::mem::take(&mut file_state);
            let scan_result = tokio::task::spawn_blocking(move || {
                let mut new_state = prev_state;
                let mut results: Vec<(PathBuf, Vec<TranscriptEntry>)> = Vec::new();

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
                                results.push((file_path.clone(), entries));
                            }
                            entry_state.size = current_size;
                        }
                    }
                }

                (new_state, results)
            })
            .await?;

            let (new_state, results) = scan_result;
            file_state = new_state;

            for (file_path, entries) in &results {
                // Transcript filenames may contain a session ID prefix that matches
                // a project session. For simplicity, we try matching all known CWDs
                // against the registry.
                let mut matched_pid: Option<u32> = None;
                let mut matched_session: Option<&ProjectSessionInfo> = None;

                for (cwd, session_info) in &cwd_to_session {
                    if let Some(pid) = registry.find_pid(AgentTool::Claude, cwd) {
                        // Check if this transcript belongs to this session by matching
                        // the session_id in the filename
                        if let Some(ref sid) = session_info.session_id {
                            if let Some(fname) = file_path.file_name().and_then(|n| n.to_str()) {
                                if fname.contains(sid) {
                                    matched_pid = Some(pid);
                                    matched_session = Some(session_info);
                                    break;
                                }
                            }
                        }
                        // Fallback: if we only have one Claude process, use it
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

                let pid = match matched_pid {
                    Some(p) => p,
                    None => continue, // no matching process discovered yet
                };

                let session_key = file_path.to_string_lossy().to_string();
                let state = session_states.entry(session_key).or_default();

                state.pid = Some(pid);

                // Emit SessionInfo once
                if !state.session_info_sent {
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
                    }
                    state.session_info_sent = true;
                }

                // Only process new entries since last check
                if entries.len() <= state.entry_count {
                    continue;
                }

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

                let new_entries = &entries[state.entry_count..];
                state.entry_count = entries.len();
                state.last_mtime = file_state.get(file_path).map(|s| s.mtime);

                // Infer status from transcript
                let inferred = infer_status_from_transcript(entries);
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

                let normalized_state = normalize_claude_state(entries, false);
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

                // Emit ActivityChanged based on what the last entry implies
                let new_activity = infer_activity_from_last_entry(entries);
                let _ = tx
                    .send(AgentEvent::ActivityChanged {
                        id: AgentId::new(AgentTool::Claude, pid),
                        pid: Some(pid),
                        activity: new_activity,
                    })
                    .await;

                // Extract todos and emit if changed
                let todos = extract_todos_from_transcript(entries);
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

                state.last_entries = entries.clone();
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
