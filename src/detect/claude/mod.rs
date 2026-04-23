use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use chrono::Utc;
use tokio::sync::mpsc;

use crate::detect::error::DetectorError;
use crate::detect::registry::RegistryHandle;
use crate::event::{AgentEvent, AgentRef};
use nexus_domain::agents::activity::Subagent;
use nexus_domain::agents::ids::{AgentId, AgentTool};
use nexus_domain::agents::status::PrimaryStatus;
use nexus_domain::shared::time::from_rfc3339;

mod inference;
mod parser;
#[cfg(test)]
mod tests;
mod types;

pub use inference::*;
pub use parser::*;
pub use types::*;

/// Seconds since last transcript write before we consider the agent plausibly
/// idle. Claude starts streaming within 2-3 s of receiving input; most silent
/// stretches beyond 10 s are tool calls or subagents, which the normalized
/// state already labels correctly as Working(ToolCall/Subagent). A shorter
/// threshold surfaces idle sessions faster without affecting active ones.
const CLAUDE_IDLE_SECS: u64 = 10;

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
    registry: RegistryHandle,
) -> Result<(), DetectorError> {
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
            tokio::task::spawn_blocking(move || scan_session_registry(&sdir))
                .await
                .map_err(|e| DetectorError::io(nexus_domain::system::DetectorKind::Claude, "spawn_blocking join", std::io::Error::other(e.to_string())))?;
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
            .await
            .map_err(|e| DetectorError::io(nexus_domain::system::DetectorKind::Claude, "spawn_blocking join", std::io::Error::other(e.to_string())))?;

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
                let mut by_cwd: HashMap<PathBuf, (PathBuf, std::time::SystemTime)> = HashMap::new();
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

            // PIDs already claimed by a project session file.  Transcript files
            // that resolve to the same PID are skipped to avoid duplicate events:
            // the project file is the authoritative source for main-session state.
            //
            // Built from `project_file_cwds` (all currently-tracked project files),
            // NOT from `results` (only files that grew this poll).  Using `results`
            // left the set empty when the main session file was idle, allowing a
            // subagent transcript to slip through and overwrite the main agent's
            // normalized state (often with `Unknown` if the subagent had only
            // `LocalCommand` entries from a `/compact` run).
            let mut project_file_pids: std::collections::HashSet<u32> =
                std::collections::HashSet::new();
            for (_, cwd) in &project_file_cwds {
                if let Some(pid) = registry.find_pid(AgentTool::Claude, cwd).await {
                    project_file_pids.insert(pid);
                }
            }

            let dir = transcripts_dir.clone();
            // Move the combined file-state map into spawn_blocking; this is an
            // O(1) pointer move, not a clone.
            let prev_state = std::mem::take(&mut file_state);
            // Each result is (file_path, entries, cwd_hint).
            // cwd_hint is Some(cwd) for project session files (use CWD-based PID
            // lookup) and None for transcript files (use session-id matching).
            let scan_result = tokio::task::spawn_blocking(move || {
                let mut new_state = prev_state;
                let mut results: Vec<(PathBuf, Vec<TranscriptEntry>, Option<PathBuf>)> = Vec::new();

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
            .await
            .map_err(|e| DetectorError::io(nexus_domain::system::DetectorKind::Claude, "spawn_blocking join", std::io::Error::other(e.to_string())))?;

            let (new_state, results) = scan_result;
            file_state = new_state;

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

                    let fallback_pid = if registry_pid.is_none() {
                        registry.find_pid(AgentTool::Claude, cwd).await
                    } else {
                        None
                    };
                    if let Some(pid) = registry_pid.or(fallback_pid) {
                        matched_pid = Some(pid);
                        matched_session = session_info;
                    }
                } else {
                    // Transcript file: match by session_id in filename, with
                    // single-process fallback.
                    for (cwd, session_info) in &cwd_to_session {
                        if let Some(pid) = registry.find_pid(AgentTool::Claude, cwd).await {
                            if let Some(ref sid) = session_info.session_id {
                                if let Some(fname) = file_path.file_name().and_then(|n| n.to_str())
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
                        let claude_agents = registry.agents_for_tool(AgentTool::Claude).await;
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
                let launch_info = registry.find_claude_launch(pid).await;
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
                                    agent: AgentRef::new(AgentId::new(AgentTool::Claude, pid), Some(pid)),
                                    session_id: session_info.session_id.clone(),
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
                            agent: AgentRef::new(AgentId::new(AgentTool::Claude, pid), Some(pid)),
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
                            agent: AgentRef::new(AgentId::new(AgentTool::Claude, pid), Some(pid)),
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
                            agent: AgentRef::new(AgentId::new(AgentTool::Claude, pid), Some(pid)),
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
                                                agent: AgentRef::new(AgentId::new(AgentTool::Claude, pid), Some(pid)),
                                                subagent: Subagent {
                                                    name: subagent_type.to_string(),
                                                    status: PrimaryStatus::Running,
                                                    reason: None,
                                                    description,
                                                    started_at: from_rfc3339(entry_timestamp)
                                                        .unwrap_or_else(Utc::now),
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
                                        agent: AgentRef::new(AgentId::new(AgentTool::Claude, pid), Some(pid)),
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
                        agent: AgentRef::new(AgentId::new(AgentTool::Claude, pid), Some(pid)),
                        activity: new_activity,
                    })
                    .await;

                // Extract todos and emit if changed
                let todos = extract_todos_from_transcript(relevant_entries);
                if todos != state.last_todos {
                    let _ = tx
                        .send(AgentEvent::TodosUpdated {
                            agent: AgentRef::new(AgentId::new(AgentTool::Claude, pid), Some(pid)),
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
                        agent: AgentRef::new(AgentId::new(AgentTool::Claude, pid), Some(pid)),
                        old,
                        new: PrimaryStatus::Waiting,
                    })
                    .await;
                let _ = tx
                    .send(AgentEvent::ActivityChanged {
                        agent: AgentRef::new(AgentId::new(AgentTool::Claude, pid), Some(pid)),
                        activity: None, // clear activity when Waiting
                    })
                    .await;

                let normalized_state = normalize_claude_state(&state.last_entries, true);
                if state.last_normalized_state.as_ref() != Some(&normalized_state) {
                    let _ = tx
                        .send(AgentEvent::NormalizedStateChanged {
                            agent: AgentRef::new(AgentId::new(AgentTool::Claude, pid), Some(pid)),
                            state: normalized_state.clone(),
                        })
                        .await;
                    state.last_normalized_state = Some(normalized_state);
                }

                state.last_status = Some(PrimaryStatus::Waiting);
            }
        }

        // Evict file_state entries whose files no longer exist on disk.
        file_state.retain(|path, _| path.exists());

        tokio::time::sleep(poll_interval).await;
    }
}
