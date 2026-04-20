use crate::detect::heuristics::looks_like_review_output;
use nexus_domain::agents::activity::Activity;
use nexus_domain::agents::status::{
    NormalizedReason, NormalizedState, NormalizedStatus, PrimaryStatus,
};

use super::types::{CodexInferredStatus, ObservedCodexSession};

// ── Status Mapping ────────────────────────────────────────────────────

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

// ── Activity Inference ────────────────────────────────────────────────

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

// ── Normalization Helpers ─────────────────────────────────────────────

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

pub(super) fn looks_like_input_request_output(text: &str) -> Option<NormalizedReason> {
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

// ── Main Normalization ────────────────────────────────────────────────

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
