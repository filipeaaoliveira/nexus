use crate::detect::heuristics::looks_like_review_output;
use crate::event::{AgentEvent, AgentRef};
use nexus_domain::agents::activity::{Activity, Subagent};
use nexus_domain::agents::ids::{AgentId, AgentTool};
use nexus_domain::agents::status::{
    NormalizedReason, NormalizedState, NormalizedStatus, PrimaryStatus,
};
use nexus_domain::shared::time::from_epoch_millis;

use super::types::{OpenCodeSession, PartData, SessionEmitState};

pub fn normalize_opencode_state(
    session_state: Option<&str>,
    _latest_tool_name: Option<&str>,
    parts: &[PartData],
    assistant_turn_completed: bool,
    latest_assistant_message: Option<&str>,
    latest_assistant_error: Option<&str>,
) -> NormalizedState {
    if session_state == Some("retry")
        || parts
            .iter()
            .any(|part| matches!(part.part_type.as_str(), "retry" | "retrying"))
    {
        return NormalizedState::new(NormalizedStatus::Working, Some(NormalizedReason::Retrying));
    }

    if parts
        .iter()
        .any(|part| matches!(part.tool_status.as_deref(), Some("running" | "pending")))
    {
        return NormalizedState::new(NormalizedStatus::Working, Some(NormalizedReason::ToolCall));
    }

    if matches!(session_state, Some("busy" | "working" | "running")) {
        return NormalizedState::new(NormalizedStatus::Working, None);
    }

    if latest_assistant_error.is_some_and(|error| !error.trim().is_empty()) {
        return NormalizedState::new(NormalizedStatus::Blocked, Some(NormalizedReason::Error));
    }

    if latest_assistant_message.is_some_and(looks_like_review_output) {
        return NormalizedState::new(NormalizedStatus::NeedsInput, Some(NormalizedReason::Review));
    }

    if assistant_turn_completed {
        return NormalizedState::new(
            NormalizedStatus::Done,
            Some(NormalizedReason::ReadyForNextTask),
        );
    }

    NormalizedState::new(NormalizedStatus::Unknown, None)
}

pub fn observed_session_state(status: PrimaryStatus) -> Option<&'static str> {
    match status {
        PrimaryStatus::Running => Some("busy"),
        PrimaryStatus::Waiting => Some("idle"),
        PrimaryStatus::Error | PrimaryStatus::Done | PrimaryStatus::Exited => None,
        _ => None,
    }
}

/// Resolve the primary status from recent parts and whether the assistant
/// turn has completed.
///
/// Decision priority:
/// 1. Any tool part with `running` or `pending` status → Running
/// 2. Latest part type determines status (most map to Running)
/// 3. `step-finish` + `assistant_turn_completed` → Waiting
/// 4. No parts + `assistant_turn_completed` → Waiting
/// 5. Otherwise → Running
pub fn resolve_status(parts: &[PartData], assistant_turn_completed: bool) -> PrimaryStatus {
    // Priority 1: any running/pending tool anywhere in the list
    if parts
        .iter()
        .any(|p| matches!(p.tool_status.as_deref(), Some("running" | "pending")))
    {
        return PrimaryStatus::Running;
    }

    // Priority 2: look at the latest part
    if let Some(latest) = parts.first() {
        return match latest.part_type.as_str() {
            "step-finish" => {
                if assistant_turn_completed {
                    PrimaryStatus::Waiting
                } else {
                    PrimaryStatus::Running
                }
            }
            // All other part types indicate active work.
            // Note: OpenCode does not emit "error" parts — errors are stored
            // in the assistant message's `data.error` field, not as part types.
            _ => PrimaryStatus::Running,
        };
    }

    // No parts at all
    if assistant_turn_completed {
        PrimaryStatus::Waiting
    } else {
        PrimaryStatus::Running
    }
}

/// Infer an Activity from recent parts.
///
/// Examines the most recent tool state first. If the newest tool part in the
/// recent window is active (`running` or `pending`), report `ToolCall`.
/// Otherwise, fall back to the latest part type so stale tool snapshots do not
/// override newer terminal states.
pub fn infer_activity_from_parts(parts: &[PartData]) -> Option<Activity> {
    // Parts are ordered newest-first. Use the newest tool state we can see,
    // rather than letting older tool snapshots override newer terminal states.
    if let Some(latest_tool) = parts
        .iter()
        .find(|p| matches!(p.part_type.as_str(), "tool" | "tool-invocation"))
    {
        if matches!(
            latest_tool.tool_status.as_deref(),
            Some("running" | "pending")
        ) {
            return Some(Activity::ToolCall);
        }
    }

    // Fall back to latest part type
    if let Some(latest) = parts.first() {
        return match latest.part_type.as_str() {
            "compaction" => Some(Activity::Compacting),
            "tool" | "tool-invocation" => {
                // Tool completed/error/pending — model is thinking about next step
                Some(Activity::Thinking)
            }
            "step-start" => Some(Activity::Thinking),
            "text" => Some(Activity::Generating),
            "step-finish" => Some(Activity::Idle),
            "reasoning" => Some(Activity::Thinking),
            _ => None,
        };
    }
    None
}

/// Check if a session title indicates it's a subagent session.
#[cfg(test)]
pub fn is_subagent_title(title: &str) -> bool {
    title.contains("subagent)")
}

/// Extract the subagent type from a title like "Fix tests (@explore subagent)".
pub fn extract_subagent_type(title: &str) -> Option<&str> {
    let start = title.find("(@")? + 2;
    let end = title[start..].find(" subagent)")? + start;
    Some(&title[start..end])
}

/// Collect [`AgentEvent`]s for a session's subagents and update `state`.
///
/// - Emits [`AgentEvent::SubagentSpawned`] the first time a subagent is seen.
/// - Emits [`AgentEvent::SubagentCompleted`] exactly once when an already-seen
///   subagent becomes archived.
pub fn collect_subagent_events(
    subagents: &[OpenCodeSession],
    state: &mut SessionEmitState,
    tool: AgentTool,
    pid: u32,
    process_started_at: Option<chrono::DateTime<chrono::Local>>,
) -> Vec<AgentEvent> {
    let mut events = Vec::new();
    let process_start_ms = process_started_at.map(|dt| dt.timestamp_millis());

    for sub in subagents {
        // Skip subagents created before the current process started.
        // This prevents historical subagents from previous runs being counted.
        if let Some(threshold) = process_start_ms {
            if sub.time_created < threshold {
                continue;
            }
        }
        if state.emitted_subagent_ids.contains(&sub.id) {
            // Already announced — check for first-time completion.
            if sub.time_archived.is_some() && !state.completed_subagent_ids.contains(&sub.id) {
                let name = sub
                    .title
                    .as_deref()
                    .and_then(extract_subagent_type)
                    .unwrap_or("unknown")
                    .to_string();
                events.push(AgentEvent::SubagentCompleted {
                    agent: AgentRef::new(AgentId::new(tool, pid), Some(pid)),
                    subagent_name: name,
                });
                state.completed_subagent_ids.insert(sub.id.clone());
            }
            continue;
        }

        // First time we see this subagent — emit SubagentSpawned.
        if let Some(ref title) = sub.title {
            let subagent_type = extract_subagent_type(title)
                .unwrap_or("unknown")
                .to_string();
            events.push(AgentEvent::SubagentSpawned {
                agent: AgentRef::new(AgentId::new(tool, pid), Some(pid)),
                subagent: Subagent {
                    name: subagent_type,
                    status: if sub.time_archived.is_some() {
                        PrimaryStatus::Done
                    } else {
                        PrimaryStatus::Running
                    },
                    reason: None,
                    description: sub.title.clone(),
                    started_at: from_epoch_millis(sub.time_created)
                        .unwrap_or_else(chrono::Local::now),
                    ended_at: None,
                },
            });
            state.emitted_subagent_ids.insert(sub.id.clone());
            // If already archived when first discovered, mark completed immediately
            // to prevent a spurious SubagentCompleted event on the very next poll.
            if sub.time_archived.is_some() {
                state.completed_subagent_ids.insert(sub.id.clone());
            }
        }
    }

    events
}
