use crate::detect::heuristics::looks_like_review_output;
use crate::detect::registry::ClaudeLaunchInfo;
use nexus_domain::agents::activity::Activity;
use nexus_domain::agents::status::{
    NormalizedReason, NormalizedState, NormalizedStatus, PrimaryStatus,
};

use super::parser::{is_compaction_restore_message, last_non_assistant_entry};
use super::types::{InferredStatus, ProjectSessionInfo, TranscriptEntry};

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
                TranscriptEntry::ToolUse { tool_name, .. } => {
                    // A stale tool call is ambiguous for Claude: many tools can
                    // run for a while without new transcript writes. Do not
                    // claim user approval is needed unless we have an explicit
                    // approval signal. Keep the session as working and preserve
                    // whether this was a regular tool call or a subagent spawn.
                    let reason = if tool_name == "Agent" {
                        NormalizedReason::Subagent
                    } else {
                        NormalizedReason::ToolCall
                    };
                    return NormalizedState::new(NormalizedStatus::Working, Some(reason));
                }
                TranscriptEntry::ToolResult { .. } => {
                    // Stale after a tool result means Claude had received the
                    // tool output but never wrote an explicit completion
                    // signal. Keep the session as working rather than
                    // announcing it is ready for the next task.
                    return NormalizedState::new(
                        NormalizedStatus::Working,
                        Some(NormalizedReason::ToolCall),
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
                    // A stale trailing user message means the session received
                    // input but produced no output for 30+ seconds. Claude
                    // starts streaming within seconds of receiving a message,
                    // so prolonged silence means the session is idle — not
                    // actively working.
                    return NormalizedState::new(
                        NormalizedStatus::Done,
                        Some(NormalizedReason::ReadyForNextTask),
                    );
                }
                TranscriptEntry::LocalCommand { .. } => {
                    // A slash command with no user intent — the session is idle
                    // and waiting for the next real message.
                    return NormalizedState::new(
                        NormalizedStatus::Done,
                        Some(NormalizedReason::ReadyForNextTask),
                    );
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

        // Empty stale transcripts do not provide defensible evidence of
        // completion. Prefer Unknown over a false-positive "ready for next
        // task" label.
        return NormalizedState::new(NormalizedStatus::Unknown, None);
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
                // A slash command with no user intent — the session is idle
                // and waiting for the next real message.
                return NormalizedState::new(
                    NormalizedStatus::Done,
                    Some(NormalizedReason::ReadyForNextTask),
                );
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

/// Map an inferred transcript status to a primary display status.
pub fn inferred_to_primary(inferred: InferredStatus) -> PrimaryStatus {
    match inferred {
        InferredStatus::Running => PrimaryStatus::Running,
        InferredStatus::Waiting => PrimaryStatus::Waiting,
        InferredStatus::ToolCall => PrimaryStatus::Running,
        InferredStatus::Unknown => PrimaryStatus::Waiting,
    }
}
