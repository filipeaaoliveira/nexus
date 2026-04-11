use std::collections::HashMap;
use std::path::{Path, PathBuf};

use chrono::Local;

use crate::detect::heuristics::looks_like_review_output;
use crate::detect::registry::AgentRegistry;
use crate::event::AgentEvent;
use crate::types::{
    from_epoch_secs, Activity, AgentId, AgentTool, CodexBackendHealth, NormalizedReason,
    NormalizedState, NormalizedStatus, PrimaryStatus,
};

use super::rollout::{
    extract_plan_todos, extract_subagent_spawns, extract_tool_calls, infer_rollout_status,
    is_running_rollout_event, parse_rollout,
};
use super::sqlite::{parse_source_field, query_all_spawn_edges, query_codex_threads};
use super::types::{
    CodexDetectionResult, CodexEventSnapshot, CodexInferredStatus, CodexSnapshot,
    CodexThreadClassification, ObservedCodexSession, ObservedCodexSubagent, ObservedCodexThread,
    ObservedToolCall, RolloutThreadSummary, SourceInfo, SpawnEdge,
};

use super::{CONSERVATIVE_PID_BINDING_WINDOW_SECS, SAME_CWD_MULTI_BIND_MAX_SKEW_SECS};

// ── Helpers ───────────────────────────────────────────────────────

/// Map a CodexInferredStatus to a PrimaryStatus.
pub(super) fn codex_to_primary(status: CodexInferredStatus) -> PrimaryStatus {
    match status {
        CodexInferredStatus::Running => PrimaryStatus::Running,
        CodexInferredStatus::Done => PrimaryStatus::Done,
        CodexInferredStatus::Error => PrimaryStatus::Error,
        CodexInferredStatus::Aborted => PrimaryStatus::Waiting,
        CodexInferredStatus::Unknown => PrimaryStatus::Running,
    }
}

pub(super) fn codex_pid_for_thread(registry: &AgentRegistry, cwd: &Path) -> Option<u32> {
    match registry.find_pids(AgentTool::Codex, cwd).as_slice() {
        [pid] => Some(*pid),
        _ => None,
    }
}

pub(super) fn codex_pid_for_exact_thread(registry: &AgentRegistry, thread_id: &str) -> Option<u32> {
    registry.find_codex_pid_for_thread(thread_id)
}

pub(super) fn codex_launch_started_at(registry: &AgentRegistry, pid: u32) -> Option<i64> {
    registry
        .find_codex_launch(pid)
        .and_then(|launch| launch.started_at.map(|started_at| started_at.timestamp()))
}

pub(super) fn thread_is_recent_enough_for_pid_binding(
    thread: &super::types::CodexThread,
) -> bool {
    let thread_timestamp = thread.created_at.max(thread.updated_at);
    if thread_timestamp <= 0 {
        return false;
    }

    let now = Local::now().timestamp();
    let age = now.saturating_sub(thread_timestamp);
    age <= CONSERVATIVE_PID_BINDING_WINDOW_SECS
}

pub(super) fn classify_codex_thread(
    thread: &super::types::CodexThread,
    edge_parent_by_child: &HashMap<&str, &str>,
) -> CodexThreadClassification {
    let source_info = thread
        .source
        .as_deref()
        .map(parse_source_field)
        .unwrap_or(SourceInfo {
            is_subagent: false,
            parent_thread_id: None,
            agent_nickname: None,
            agent_role: None,
        });
    let parent_thread_id = source_info.parent_thread_id.or_else(|| {
        edge_parent_by_child
            .get(thread.id.as_str())
            .map(|id| (*id).to_string())
    });

    if source_info.is_subagent || parent_thread_id.is_some() {
        CodexThreadClassification::Child { parent_thread_id }
    } else {
        CodexThreadClassification::TopLevel
    }
}

pub fn build_codex_snapshot(
    threads: &[super::types::CodexThread],
    spawn_edges: &[SpawnEdge],
    registry: &AgentRegistry,
) -> CodexSnapshot {
    let edge_parent_by_child: HashMap<&str, &str> = spawn_edges
        .iter()
        .map(|edge| {
            (
                edge.child_thread_id.as_str(),
                edge.parent_thread_id.as_str(),
            )
        })
        .collect();

    let classified_threads = threads
        .iter()
        .map(|thread| {
            (
                thread.clone(),
                classify_codex_thread(thread, &edge_parent_by_child),
            )
        })
        .collect::<Vec<_>>();
    let mut top_level_bindable_thread_count_by_cwd: HashMap<String, usize> = HashMap::new();
    for (thread, classification) in &classified_threads {
        if matches!(classification, CodexThreadClassification::TopLevel)
            && thread_is_recent_enough_for_pid_binding(thread)
        {
            if let Some(cwd) = thread.cwd.as_deref() {
                *top_level_bindable_thread_count_by_cwd
                    .entry(cwd.to_string())
                    .or_default() += 1;
            }
        }
    }

    let mut fallback_pid_by_thread_id: HashMap<String, u32> = HashMap::new();
    let mut exact_bound_pids = Vec::new();

    for (thread, classification) in &classified_threads {
        if matches!(classification, CodexThreadClassification::TopLevel) {
            if let Some(pid) = codex_pid_for_exact_thread(registry, &thread.id) {
                fallback_pid_by_thread_id.insert(thread.id.clone(), pid);
                exact_bound_pids.push(pid);
            }
        }
    }

    let mut bindable_threads_by_cwd: HashMap<String, Vec<&super::types::CodexThread>> =
        HashMap::new();
    for (thread, classification) in &classified_threads {
        if matches!(classification, CodexThreadClassification::TopLevel)
            && thread_is_recent_enough_for_pid_binding(thread)
            && !fallback_pid_by_thread_id.contains_key(&thread.id)
        {
            if let Some(cwd) = thread.cwd.as_deref() {
                bindable_threads_by_cwd
                    .entry(cwd.to_string())
                    .or_default()
                    .push(thread);
            }
        }
    }

    for (cwd, candidates) in bindable_threads_by_cwd {
        let available_pids = registry
            .find_pids(AgentTool::Codex, Path::new(&cwd))
            .into_iter()
            .filter(|pid| !exact_bound_pids.contains(pid))
            .collect::<Vec<_>>();

        if available_pids.is_empty() {
            continue;
        }

        if candidates.len() == 1 && available_pids.len() == 1 {
            fallback_pid_by_thread_id.insert(candidates[0].id.clone(), available_pids[0]);
            continue;
        }

        if candidates.len() != available_pids.len() {
            continue;
        }

        let mut threads_with_times = candidates
            .into_iter()
            .map(|thread| (thread, thread.created_at))
            .collect::<Vec<_>>();
        let mut pids_with_times = available_pids
            .into_iter()
            .filter_map(|pid| {
                codex_launch_started_at(registry, pid).map(|started_at| (pid, started_at))
            })
            .collect::<Vec<_>>();

        if threads_with_times.is_empty() || pids_with_times.is_empty() {
            continue;
        }

        threads_with_times.sort_by_key(|(_, created_at)| *created_at);
        pids_with_times.sort_by_key(|(_, started_at)| *started_at);

        let mut used_thread_ids = Vec::new();
        for (pid, started_at) in pids_with_times {
            let best_match = threads_with_times
                .iter()
                .enumerate()
                .filter(|(_, (thread, _))| !used_thread_ids.contains(&thread.id))
                .map(|(idx, (thread, created_at))| {
                    let skew = (*created_at - started_at).abs();
                    (idx, thread.id.clone(), skew)
                })
                .min_by_key(|(_, _, skew)| *skew);

            let Some((_, thread_id, skew)) = best_match else {
                continue;
            };
            if skew > SAME_CWD_MULTI_BIND_MAX_SKEW_SECS {
                continue;
            }

            fallback_pid_by_thread_id.insert(thread_id.clone(), pid);
            used_thread_ids.push(thread_id);
        }
    }

    let mut snapshot = CodexSnapshot::default();

    for (thread, classification) in classified_threads {
        let pid = match &classification {
            CodexThreadClassification::TopLevel => fallback_pid_by_thread_id
                .get(&thread.id)
                .copied()
                .or_else(|| {
                    thread_is_recent_enough_for_pid_binding(&thread)
                        .then_some(())
                        .and(thread.cwd.as_deref())
                        .and_then(|cwd| {
                            (top_level_bindable_thread_count_by_cwd.get(cwd) == Some(&1))
                                .then(|| codex_pid_for_thread(registry, Path::new(cwd)))
                                .flatten()
                        })
                }),
            CodexThreadClassification::Child { .. } => None,
        };
        let observed = ObservedCodexThread {
            agent_id: AgentId::session(AgentTool::Codex, thread.id.clone()),
            pid,
            classification: classification.clone(),
            thread,
        };

        match classification {
            CodexThreadClassification::TopLevel => snapshot.top_level_threads.push(observed),
            CodexThreadClassification::Child { .. } => snapshot.child_threads.push(observed),
        }
    }

    snapshot
}

#[cfg(test)]
pub(super) fn load_spawn_edges_snapshot(db_path: &Path) -> Vec<SpawnEdge> {
    match query_all_spawn_edges(db_path) {
        Ok(edges) => edges,
        Err(error) => {
            tracing::debug!("failed to load codex spawn edges: {error}");
            Vec::new()
        }
    }
}

fn degraded_backend_session(reason: &str) -> ObservedCodexSession {
    ObservedCodexSession {
        agent_id: AgentId::session(AgentTool::Codex, "codex-backend"),
        pid: Some(0),
        cwd: PathBuf::from("/"),
        branch: None,
        started_at: None,
        status: PrimaryStatus::Error,
        final_output: Some(format!("Codex backend is degraded: {reason}")),
        final_output_is_completion: false,
        latest_tool_call: None,
        todos: Vec::new(),
        subagents: Vec::new(),
    }
}

fn materialize_backend_health(
    mut snapshot: CodexEventSnapshot,
    backend_health: &CodexBackendHealth,
) -> CodexEventSnapshot {
    if let CodexBackendHealth::Degraded { reason } = backend_health {
        snapshot
            .sessions
            .insert(0, degraded_backend_session(reason));
    }

    snapshot
}

pub fn detect_codex_backend(db_path: &Path, registry: &AgentRegistry) -> CodexDetectionResult {
    detect_codex_backend_with_history(db_path, registry, false)
}

pub fn detect_codex_backend_with_history(
    db_path: &Path,
    registry: &AgentRegistry,
    has_previous_state: bool,
) -> CodexDetectionResult {
    if !db_path.exists() {
        if has_previous_state {
            let backend_health = CodexBackendHealth::degraded("sqlite state is missing");
            return CodexDetectionResult {
                snapshot: materialize_backend_health(
                    CodexEventSnapshot::default(),
                    &backend_health,
                ),
                backend_health,
            };
        }

        return CodexDetectionResult {
            backend_health: CodexBackendHealth::Healthy,
            snapshot: CodexEventSnapshot::default(),
        };
    }

    let threads = match query_codex_threads(db_path) {
        Ok(threads) => threads,
        Err(error) => {
            let backend_health =
                CodexBackendHealth::degraded(format!("sqlite threads query failed: {error}"));
            return CodexDetectionResult {
                snapshot: materialize_backend_health(
                    CodexEventSnapshot::default(),
                    &backend_health,
                ),
                backend_health,
            };
        }
    };

    let mut backend_health = CodexBackendHealth::Healthy;
    let spawn_edges = match query_all_spawn_edges(db_path) {
        Ok(edges) => edges,
        Err(error) => {
            backend_health =
                CodexBackendHealth::degraded(format!("sqlite spawn edges query failed: {error}"));
            Vec::new()
        }
    };

    let snapshot = build_codex_snapshot(&threads, &spawn_edges, registry);
    let snapshot = materialize_backend_health(
        build_codex_event_snapshot(&snapshot, &spawn_edges),
        &backend_health,
    );

    CodexDetectionResult {
        backend_health,
        snapshot,
    }
}

pub(super) fn snapshot_child_threads_by_parent(
    snapshot: &CodexSnapshot,
) -> HashMap<String, Vec<ObservedCodexThread>> {
    let mut grouped = HashMap::new();

    for child in &snapshot.child_threads {
        if let CodexThreadClassification::Child {
            parent_thread_id: Some(parent_thread_id),
        } = &child.classification
        {
            grouped
                .entry(parent_thread_id.clone())
                .or_insert_with(Vec::new)
                .push(child.clone());
        }
    }

    grouped
}

fn summarize_tool_arguments(arguments: &str) -> Option<String> {
    if arguments == "{}" {
        None
    } else if arguments.chars().count() > 100 {
        Some(arguments.chars().take(100).collect())
    } else {
        Some(arguments.to_string())
    }
}

fn extract_final_output_text(events: &[super::types::RolloutEvent]) -> (Option<String>, bool) {
    if let Some(task_complete_index) = events.iter().rposition(|event| {
        event.event_type == "event_msg"
            && event.payload.get("type").and_then(|value| value.as_str()) == Some("task_complete")
    }) {
        let has_newer_activity = events
            .iter()
            .skip(task_complete_index + 1)
            .any(is_running_rollout_event);
        if has_newer_activity {
            return (None, false);
        }

        return (
            events[task_complete_index]
                .payload
                .get("last_agent_message")
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|text| !text.is_empty())
                .map(str::to_string),
            true,
        );
    }

    (
        events.iter().rev().find_map(|event| {
            if event.event_type != "response_item" {
                return None;
            }

            let payload = &event.payload;
            if payload.get("type").and_then(|value| value.as_str()) != Some("message") {
                return None;
            }

            payload
                .get("content")
                .and_then(|value| value.as_array())
                .and_then(|content| {
                    let parts = content
                        .iter()
                        .filter(|item| {
                            item.get("type").and_then(|value| value.as_str()) == Some("output_text")
                        })
                        .filter_map(|item| item.get("text").and_then(|value| value.as_str()))
                        .map(str::trim)
                        .filter(|text| !text.is_empty())
                        .collect::<Vec<_>>();
                    (!parts.is_empty()).then(|| parts.join("\n"))
                })
        }),
        false,
    )
}

fn extract_latest_active_tool_call(
    events: &[super::types::RolloutEvent],
) -> Option<super::types::ToolCallInfo> {
    let relevant_events = if let Some(task_complete_index) = events.iter().rposition(|event| {
        event.event_type == "event_msg"
            && event.payload.get("type").and_then(|value| value.as_str()) == Some("task_complete")
    }) {
        let has_newer_activity = events
            .iter()
            .skip(task_complete_index + 1)
            .any(is_running_rollout_event);
        if has_newer_activity {
            &events[task_complete_index + 1..]
        } else {
            return None;
        }
    } else {
        events
    };

    extract_tool_calls(relevant_events).last().cloned()
}


fn looks_like_input_request_output(text: &str) -> Option<NormalizedReason> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }

    let lower = trimmed.to_ascii_lowercase();

    if [
        "need your approval",
        "awaiting your approval",
        "waiting for your approval",
        "please approve",
        "can you approve",
        "do you approve",
        "please confirm",
        "can you confirm",
        "confirm before i continue",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
    {
        return Some(NormalizedReason::Approval);
    }

    if trimmed.contains('?')
        && [
            "which option",
            "which approach",
            "which direction",
            "which one",
            "what do you want",
            "how would you like",
            "do you want me to",
            "would you like me to",
            "should i",
            "let me know",
            "waiting on your input",
            "need your input",
            "need your decision",
        ]
        .iter()
        .any(|marker| lower.contains(marker))
    {
        return Some(NormalizedReason::Decision);
    }

    None
}

fn looks_like_ready_for_next_task_output(text: &str) -> bool {
    let lower = text.trim().to_ascii_lowercase();
    [
        "ready for the next task",
        "ready for next task",
        "implementation complete",
        "changes are complete",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
}

/// Infer the current `Activity` for a codex session from its observed state.
///
/// - Running + active tool call → ToolCall
/// - Running, no tool call, running subagent → ToolCall (subagent is using tools)
/// - Running, nothing active → Generating (model is thinking/writing)
/// - Not Running → Idle
pub(super) fn infer_codex_activity(session: &ObservedCodexSession) -> Option<Activity> {
    match session.status {
        PrimaryStatus::Running => {
            if session.latest_tool_call.is_some()
                || session
                    .subagents
                    .iter()
                    .any(|s| s.status == PrimaryStatus::Running)
            {
                Some(Activity::ToolCall)
            } else if session.final_output.is_some() {
                Some(Activity::Idle)
            } else {
                Some(Activity::Generating)
            }
        }
        _ => Some(Activity::Idle),
    }
}

fn waiting_subagent_reason(session: &ObservedCodexSession) -> Option<NormalizedReason> {
    session
        .subagents
        .iter()
        .find(|subagent| {
            matches!(
                subagent.status,
                PrimaryStatus::Waiting | PrimaryStatus::Done
            ) && subagent.reason.is_some()
        })
        .and_then(|subagent| subagent.reason.clone())
}

pub fn normalize_codex_session(
    session: &ObservedCodexSession,
    final_output: Option<&str>,
) -> NormalizedState {
    let has_meaningful_final_output = final_output
        .map(str::trim)
        .is_some_and(|output| !output.is_empty());

    if session.status == PrimaryStatus::Done && final_output.is_some_and(looks_like_review_output) {
        return NormalizedState::new(NormalizedStatus::NeedsInput, Some(NormalizedReason::Review));
    }

    match session.status {
        PrimaryStatus::Error => {
            NormalizedState::new(NormalizedStatus::Blocked, Some(NormalizedReason::Error))
        }
        PrimaryStatus::Running => {
            if session.latest_tool_call.is_some() {
                NormalizedState::new(NormalizedStatus::Working, Some(NormalizedReason::ToolCall))
            } else if let Some(reason) = waiting_subagent_reason(session) {
                NormalizedState::new(NormalizedStatus::NeedsInput, Some(reason))
            } else if session
                .subagents
                .iter()
                .any(|subagent| subagent.status == PrimaryStatus::Running)
            {
                NormalizedState::new(NormalizedStatus::Working, Some(NormalizedReason::Subagent))
            } else if final_output.is_some_and(looks_like_ready_for_next_task_output)
                || (has_meaningful_final_output
                    && session.final_output_is_completion
                    && !final_output.is_some_and(looks_like_review_output))
            {
                NormalizedState::new(
                    NormalizedStatus::Done,
                    Some(NormalizedReason::ReadyForNextTask),
                )
            } else {
                NormalizedState::new(NormalizedStatus::Working, None)
            }
        }
        PrimaryStatus::Waiting => {
            if let Some(reason) = final_output.and_then(looks_like_input_request_output) {
                NormalizedState::new(NormalizedStatus::NeedsInput, Some(reason))
            } else if let Some(reason) = waiting_subagent_reason(session) {
                NormalizedState::new(NormalizedStatus::NeedsInput, Some(reason))
            } else if session
                .subagents
                .iter()
                .any(|subagent| subagent.status == PrimaryStatus::Running)
            {
                NormalizedState::new(NormalizedStatus::Working, Some(NormalizedReason::Subagent))
            } else if session.latest_tool_call.is_some() {
                NormalizedState::new(NormalizedStatus::Working, Some(NormalizedReason::ToolCall))
            } else {
                NormalizedState::new(NormalizedStatus::Unknown, None)
            }
        }
        PrimaryStatus::Done => {
            if let Some(reason) = final_output.and_then(looks_like_input_request_output) {
                NormalizedState::new(NormalizedStatus::NeedsInput, Some(reason))
            } else if let Some(reason) = waiting_subagent_reason(session) {
                NormalizedState::new(NormalizedStatus::NeedsInput, Some(reason))
            } else if session
                .subagents
                .iter()
                .any(|subagent| subagent.status == PrimaryStatus::Running)
            {
                NormalizedState::new(NormalizedStatus::Working, Some(NormalizedReason::Subagent))
            } else if has_meaningful_final_output {
                NormalizedState::new(
                    NormalizedStatus::Done,
                    Some(NormalizedReason::ReadyForNextTask),
                )
            } else {
                NormalizedState::new(NormalizedStatus::Unknown, None)
            }
        }
        PrimaryStatus::Exited => NormalizedState::new(NormalizedStatus::Unknown, None),
    }
}

fn rollout_summary_for_thread(thread: &super::types::CodexThread) -> RolloutThreadSummary {
    let mut summary = RolloutThreadSummary {
        status: PrimaryStatus::Waiting,
        final_output: None,
        final_output_is_completion: false,
        latest_tool_call: None,
        todos: Vec::new(),
        spawned_subagents: Vec::new(),
    };

    if let Some(ref rollout_path) = thread.rollout_path {
        let path = PathBuf::from(rollout_path);
        if path.exists() {
            if let Ok(content) = std::fs::read_to_string(&path) {
                let events = parse_rollout(&content);
                summary.status = codex_to_primary(infer_rollout_status(&events));
                let (final_output, final_output_is_completion) =
                    extract_final_output_text(&events);
                summary.final_output = final_output;
                summary.final_output_is_completion = final_output_is_completion;
                summary.latest_tool_call =
                    extract_latest_active_tool_call(&events).map(|tool| ObservedToolCall {
                        name: tool.name.clone(),
                        args_summary: summarize_tool_arguments(&tool.arguments),
                    });
                summary.todos = extract_plan_todos(&events);
                summary.spawned_subagents = extract_subagent_spawns(&events)
                    .into_iter()
                    .map(|spawn| {
                        let thread_id = spawn.thread_id;
                        ObservedCodexSubagent {
                            name: spawn.nickname,
                            thread_id: thread_id.clone(),
                            description: Some(thread_id),
                            status: PrimaryStatus::Waiting,
                            reason: None,
                            started_at: None,
                        }
                    })
                    .collect();
            }
        }
    }

    summary
}

fn child_thread_subagent_metadata(
    child: &ObservedCodexThread,
) -> (String, Option<String>) {
    let source_info = child
        .thread
        .source
        .as_deref()
        .map(parse_source_field)
        .unwrap_or(SourceInfo {
            is_subagent: false,
            parent_thread_id: None,
            agent_nickname: None,
            agent_role: None,
        });
    let name = child
        .thread
        .agent_nickname
        .clone()
        .or(source_info.agent_nickname)
        .unwrap_or_else(|| child.thread.id.clone());

    (name, Some(child.thread.id.clone()))
}

fn child_thread_state(
    child: &ObservedCodexThread,
    edge_status: Option<&str>,
) -> (PrimaryStatus, Option<NormalizedReason>) {
    let edge_indicates_closed = matches!(edge_status, Some("completed" | "closed"));

    if let Some(ref rollout_path) = child.thread.rollout_path {
        let rp = PathBuf::from(rollout_path);
        if rp.exists() {
            if let Ok(content) = std::fs::read_to_string(&rp) {
                let events = parse_rollout(&content);
                let (final_output, _) = extract_final_output_text(&events);
                let reason = final_output
                    .as_deref()
                    .and_then(looks_like_input_request_output);

                let status = match infer_rollout_status(&events) {
                    CodexInferredStatus::Running => PrimaryStatus::Running,
                    CodexInferredStatus::Done => PrimaryStatus::Done,
                    CodexInferredStatus::Error => PrimaryStatus::Error,
                    CodexInferredStatus::Aborted if edge_indicates_closed => PrimaryStatus::Done,
                    CodexInferredStatus::Aborted => PrimaryStatus::Waiting,
                    CodexInferredStatus::Unknown if edge_indicates_closed => PrimaryStatus::Done,
                    CodexInferredStatus::Unknown => PrimaryStatus::Waiting,
                };

                return (status, reason);
            }
        }
    }

    (
        match edge_status {
            Some("completed" | "closed") => PrimaryStatus::Done,
            _ => PrimaryStatus::Waiting,
        },
        None,
    )
}

fn observed_subagents_for_parent(
    parent: &ObservedCodexThread,
    child_threads_by_parent: &HashMap<String, Vec<ObservedCodexThread>>,
    spawn_status_by_child: &HashMap<&str, &str>,
    rollout_subagents: Vec<ObservedCodexSubagent>,
) -> Vec<ObservedCodexSubagent> {
    let mut subagents_by_thread_id: HashMap<String, ObservedCodexSubagent> =
        child_threads_by_parent
            .get(&parent.thread.id)
            .into_iter()
            .flatten()
            .map(|child| {
                let (name, description) = child_thread_subagent_metadata(child);
                let (status, reason) = child_thread_state(
                    child,
                    spawn_status_by_child.get(child.thread.id.as_str()).copied(),
                );

                ObservedCodexSubagent {
                    thread_id: child.thread.id.clone(),
                    name,
                    description,
                    status,
                    reason,
                    started_at: crate::types::from_epoch_secs(child.thread.created_at),
                }
            })
            .map(|subagent| (subagent.thread_id.clone(), subagent))
            .collect();

    for subagent in rollout_subagents {
        subagents_by_thread_id
            .entry(subagent.thread_id.clone())
            .or_insert(subagent);
    }

    let mut subagents = subagents_by_thread_id.into_values().collect::<Vec<_>>();
    subagents.sort_by(|a, b| a.thread_id.cmp(&b.thread_id));
    subagents
}

pub fn build_codex_event_snapshot(
    snapshot: &CodexSnapshot,
    spawn_edges: &[SpawnEdge],
) -> CodexEventSnapshot {
    let child_threads_by_parent = snapshot_child_threads_by_parent(snapshot);
    let spawn_status_by_child: HashMap<&str, &str> = spawn_edges
        .iter()
        .map(|edge| (edge.child_thread_id.as_str(), edge.status.as_str()))
        .collect();
    let mut sessions = snapshot
        .top_level_threads
        .iter()
        .map(|observed| {
            let cwd = observed
                .thread
                .cwd
                .as_deref()
                .map(PathBuf::from)
                .unwrap_or_default();
            let rollout_summary = rollout_summary_for_thread(&observed.thread);
            let live_between_turns =
                observed.pid.is_some() && rollout_summary.status == PrimaryStatus::Done;
            let status = if live_between_turns {
                PrimaryStatus::Running
            } else {
                rollout_summary.status
            };
            let todos = if live_between_turns {
                Vec::new()
            } else {
                rollout_summary.todos
            };
            let subagents = if live_between_turns {
                Vec::new()
            } else {
                observed_subagents_for_parent(
                    observed,
                    &child_threads_by_parent,
                    &spawn_status_by_child,
                    rollout_summary.spawned_subagents,
                )
            };

            ObservedCodexSession {
                agent_id: observed.agent_id.clone(),
                pid: observed.pid,
                cwd,
                branch: observed.thread.git_branch.clone(),
                started_at: from_epoch_secs(observed.thread.created_at),
                status,
                final_output: rollout_summary.final_output,
                final_output_is_completion: rollout_summary.final_output_is_completion,
                latest_tool_call: rollout_summary.latest_tool_call,
                todos,
                subagents,
            }
        })
        .collect::<Vec<_>>();
    sessions.sort_by(|a, b| a.agent_id.session_id().cmp(&b.agent_id.session_id()));

    CodexEventSnapshot { sessions }
}

pub fn reconcile_codex_snapshots(
    previous: Option<&CodexEventSnapshot>,
    current: &CodexEventSnapshot,
) -> Vec<AgentEvent> {
    let previous_sessions: HashMap<&str, &ObservedCodexSession> = previous
        .into_iter()
        .flat_map(|snapshot| snapshot.sessions.iter())
        .filter_map(|session| session.agent_id.session_id().map(|id| (id, session)))
        .collect();
    let mut events = Vec::new();

    for session in &current.sessions {
        let Some(session_id) = session.agent_id.session_id() else {
            continue;
        };
        let previous_session = previous_sessions.get(session_id).copied();

        if previous_session.is_none() {
            events.push(AgentEvent::Discovered {
                id: session.agent_id.clone(),
                pid: session.pid,
                cwd: session.cwd.clone(),
                started_at: session.started_at,
            });
        }

        let session_info_changed = previous_session.is_none_or(|previous_session| {
            previous_session.pid != session.pid
                || previous_session.branch != session.branch
                || previous_session.started_at != session.started_at
        });
        if session_info_changed {
            events.push(AgentEvent::SessionInfo {
                id: session.agent_id.clone(),
                pid: session.pid,
                session_id: session_id.to_string(),
                branch: session.branch.clone(),
                is_worktree: false,
                started_at: session.started_at,
            });
        }

        let previous_status = previous_session.map(|session| session.status);
        if previous_status != Some(session.status)
            && !(previous_status.is_none() && session.status == PrimaryStatus::Running)
        {
            events.push(AgentEvent::StatusChanged {
                id: session.agent_id.clone(),
                pid: session.pid,
                old: previous_status.unwrap_or(PrimaryStatus::Running),
                new: session.status,
            });
        }

        let normalized_state = normalize_codex_session(session, session.final_output.as_deref());
        let previous_normalized_state = previous_session.map(|previous_session| {
            normalize_codex_session(previous_session, previous_session.final_output.as_deref())
        });
        if previous_normalized_state.as_ref() != Some(&normalized_state) {
            events.push(AgentEvent::NormalizedStateChanged {
                id: session.agent_id.clone(),
                pid: session.pid,
                state: normalized_state,
            });
        }

        let previous_todos = previous_session.map(|session| &session.todos);
        if previous_todos != Some(&session.todos) {
            events.push(AgentEvent::TodosUpdated {
                id: session.agent_id.clone(),
                pid: session.pid,
                todos: session.todos.clone(),
            });
        }

        let current_activity = infer_codex_activity(session);
        let previous_activity = previous_session.map(infer_codex_activity);
        // Only emit when the activity actually changed.  For newly discovered
        // sessions (previous_session = None) suppress the event when the
        // session is already idle — there is no meaningful transition to
        // announce and it avoids a spurious event flood on startup.
        let activity_changed = match previous_activity {
            Some(ref prev) => prev != &current_activity,
            None => current_activity != Some(Activity::Idle),
        };
        if activity_changed {
            events.push(AgentEvent::ActivityChanged {
                id: session.agent_id.clone(),
                pid: session.pid,
                activity: current_activity,
            });
        }

        let previous_tool_call =
            previous_session.and_then(|session| session.latest_tool_call.as_ref());
        match (previous_tool_call, session.latest_tool_call.as_ref()) {
            (Some(previous_tool_call), Some(current_tool_call))
                if previous_tool_call != current_tool_call =>
            {
                events.push(AgentEvent::ToolCallCompleted {
                    id: session.agent_id.clone(),
                    pid: session.pid,
                });
                events.push(AgentEvent::ToolCallStarted {
                    id: session.agent_id.clone(),
                    pid: session.pid,
                    tool_name: current_tool_call.name.clone(),
                    args_summary: current_tool_call.args_summary.clone(),
                });
            }
            (Some(_previous_tool_call), None) => {
                events.push(AgentEvent::ToolCallCompleted {
                    id: session.agent_id.clone(),
                    pid: session.pid,
                });
            }
            (None, Some(current_tool_call)) => {
                events.push(AgentEvent::ToolCallStarted {
                    id: session.agent_id.clone(),
                    pid: session.pid,
                    tool_name: current_tool_call.name.clone(),
                    args_summary: current_tool_call.args_summary.clone(),
                });
            }
            _ => {}
        }

        let previous_subagents: HashMap<&str, &ObservedCodexSubagent> = previous_session
            .into_iter()
            .flat_map(|session| session.subagents.iter())
            .map(|subagent| (subagent.thread_id.as_str(), subagent))
            .collect();

        let current_subagents = session
            .subagents
            .iter()
            .map(|subagent| crate::types::Subagent {
                name: subagent.name.clone(),
                status: subagent.status,
                reason: subagent.reason.clone(),
                description: subagent.description.clone(),
                started_at: Local::now(),
                ended_at: (subagent.status == PrimaryStatus::Done).then(Local::now),
            })
            .collect::<Vec<_>>();

        let previous_subagents_vec = previous_session
            .map(|session| {
                session
                    .subagents
                    .iter()
                    .map(|subagent| {
                        (
                            subagent.thread_id.as_str(),
                            &subagent.name,
                            subagent.status,
                            subagent.reason.as_ref(),
                            subagent.description.as_ref(),
                        )
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let current_subagents_vec = session
            .subagents
            .iter()
            .map(|subagent| {
                (
                    subagent.thread_id.as_str(),
                    &subagent.name,
                    subagent.status,
                    subagent.reason.as_ref(),
                    subagent.description.as_ref(),
                )
            })
            .collect::<Vec<_>>();
        if previous_subagents_vec != current_subagents_vec {
            events.push(AgentEvent::SubagentsUpdated {
                id: session.agent_id.clone(),
                pid: session.pid,
                subagents: current_subagents,
            });
        }

        for subagent in &session.subagents {
            match previous_subagents.get(subagent.thread_id.as_str()).copied() {
                None => {
                    events.push(AgentEvent::SubagentSpawned {
                        id: session.agent_id.clone(),
                        pid: session.pid,
                        subagent: crate::types::Subagent {
                            name: subagent.name.clone(),
                            status: subagent.status,
                            reason: subagent.reason.clone(),
                            description: subagent.description.clone(),
                            started_at: subagent.started_at.unwrap_or_else(Local::now),
                            ended_at: (subagent.status == PrimaryStatus::Done).then(Local::now),
                        },
                    });
                }
                Some(previous_subagent)
                    if previous_subagent.status != PrimaryStatus::Done
                        && subagent.status == PrimaryStatus::Done =>
                {
                    events.push(AgentEvent::SubagentCompleted {
                        id: session.agent_id.clone(),
                        pid: session.pid,
                        subagent_name: subagent.thread_id.clone(),
                    });
                }
                _ => {}
            }
        }
    }

    if previous_sessions.contains_key("codex-backend")
        && !current
            .sessions
            .iter()
            .any(|session| session.agent_id.session_id() == Some("codex-backend"))
    {
        events.push(AgentEvent::ProcessExited {
            id: AgentId::session(AgentTool::Codex, "codex-backend"),
            pid: Some(0),
        });
    }

    events
}
