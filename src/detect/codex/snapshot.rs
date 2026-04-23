use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::detect::registry::RegistrySnapshot;
use nexus_domain::agents::ids::{AgentId, AgentTool};
use nexus_domain::agents::status::{NormalizedReason, PrimaryStatus};
use nexus_domain::shared::time::from_epoch_secs;
use nexus_domain::system::health::CodexBackendHealth;

use super::normalization::{codex_to_primary, looks_like_input_request_output};
use super::pid_binding::{
    bind_pids_to_threads, classify_codex_thread, codex_pid_for_thread,
    thread_is_recent_enough_for_pid_binding,
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

// ── Helpers ───────────────────────────────────────────────────────

pub fn build_codex_snapshot(
    threads: &[super::types::CodexThread],
    spawn_edges: &[SpawnEdge],
    registry: &RegistrySnapshot,
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

    let fallback_pid_by_thread_id = bind_pids_to_threads(&classified_threads, registry);

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

#[cfg(test)]
pub fn detect_codex_backend(db_path: &Path, registry: &RegistrySnapshot) -> CodexDetectionResult {
    detect_codex_backend_with_history(db_path, registry, false)
}

pub fn detect_codex_backend_with_history(
    db_path: &Path,
    registry: &RegistrySnapshot,
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
                let (final_output, final_output_is_completion) = extract_final_output_text(&events);
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

fn child_thread_subagent_metadata(child: &ObservedCodexThread) -> (String, Option<String>) {
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
                    started_at: from_epoch_secs(child.thread.created_at),
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
