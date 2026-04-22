use serde_json::Value;
use tracing::debug;

use nexus_domain::agents::activity::{TodoItem, TodoPriority, TodoStatus};

use super::types::SessionIndexEntry;
#[cfg(test)]
use super::types::SessionMeta;
use super::types::{CodexInferredStatus, RolloutEvent, SubagentSpawn, ToolCallInfo};

// ── JSONL Parsing ─────────────────────────────────────────────────

/// Parse a Codex rollout JSONL string into a list of events.
///
/// Each line is a JSON object with `timestamp`, `type`, and `payload` fields.
pub fn parse_rollout(content: &str) -> Vec<RolloutEvent> {
    let mut events = Vec::new();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let obj: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(e) => {
                debug!("failed to parse rollout line: {e}");
                continue;
            }
        };

        let timestamp = obj
            .get("timestamp")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let event_type = obj
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let payload = obj.get("payload").cloned().unwrap_or(Value::Null);

        events.push(RolloutEvent {
            timestamp,
            event_type,
            payload,
        });
    }

    events
}

/// Parse a Codex session index JSONL string into a list of entries.
///
/// Each line is a JSON object with `id`, `thread_name`, and `updated_at`.
pub fn parse_session_index(content: &str) -> Vec<SessionIndexEntry> {
    let mut entries = Vec::new();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let obj: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(e) => {
                debug!("failed to parse session index line: {e}");
                continue;
            }
        };

        let id = match obj.get("id").and_then(|v| v.as_str()) {
            Some(s) => s.to_string(),
            None => continue,
        };

        let thread_name = obj
            .get("thread_name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let updated_at = obj
            .get("updated_at")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        entries.push(SessionIndexEntry {
            id,
            thread_name,
            updated_at,
        });
    }

    entries
}

// ── Extraction Functions ──────────────────────────────────────────

/// Extract session metadata from the first `session_meta` event in a rollout.
#[cfg(test)]
pub fn extract_session_meta(events: &[RolloutEvent]) -> Option<SessionMeta> {
    let event = events.iter().find(|e| e.event_type == "session_meta")?;

    let payload = &event.payload;

    let id = payload.get("id")?.as_str()?.to_string();
    let cwd = payload.get("cwd")?.as_str()?.to_string();
    let source = payload
        .get("source")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let model_provider = payload
        .get("model_provider")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let branch = payload
        .get("git")
        .and_then(|g| g.get("branch"))
        .and_then(|v| v.as_str())
        .map(String::from);

    Some(SessionMeta {
        id,
        cwd,
        branch,
        source,
        model_provider,
    })
}

/// Extract subagent spawn events from a rollout.
///
/// Looks for `event_msg` entries whose payload type is `collab_agent_spawn_end`.
pub fn extract_subagent_spawns(events: &[RolloutEvent]) -> Vec<SubagentSpawn> {
    events
        .iter()
        .filter(|e| e.event_type == "event_msg")
        .filter(|e| {
            e.payload.get("type").and_then(|v| v.as_str()) == Some("collab_agent_spawn_end")
        })
        .filter_map(|e| {
            let p = &e.payload;
            Some(SubagentSpawn {
                nickname: p.get("new_agent_nickname")?.as_str()?.to_string(),
                role: p.get("new_agent_role")?.as_str()?.to_string(),
                thread_id: p.get("new_thread_id")?.as_str()?.to_string(),
                prompt: p
                    .get("prompt")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
            })
        })
        .collect()
}

/// Extract tool calls from a rollout.
///
/// Looks for `response_item` entries whose payload type is `function_call`.
pub fn extract_tool_calls(events: &[RolloutEvent]) -> Vec<ToolCallInfo> {
    events
        .iter()
        .filter(|e| e.event_type == "response_item")
        .filter(|e| e.payload.get("type").and_then(|v| v.as_str()) == Some("function_call"))
        .filter_map(|e| {
            let p = &e.payload;
            Some(ToolCallInfo {
                name: p.get("name")?.as_str()?.to_string(),
                arguments: p
                    .get("arguments")
                    .and_then(|v| v.as_str())
                    .unwrap_or("{}")
                    .to_string(),
            })
        })
        .collect()
}

pub fn extract_plan_todos(events: &[RolloutEvent]) -> Vec<TodoItem> {
    events
        .iter()
        .rev()
        .find_map(|event| {
            if event.event_type != "response_item" {
                return None;
            }

            let payload = &event.payload;
            if payload.get("type").and_then(|v| v.as_str()) != Some("function_call")
                || payload.get("name").and_then(|v| v.as_str()) != Some("update_plan")
            {
                return None;
            }

            let arguments = payload.get("arguments").and_then(|v| v.as_str())?;
            let value: Value = serde_json::from_str(arguments).ok()?;
            let plan = value.get("plan")?.as_array()?;

            Some(
                plan.iter()
                    .filter_map(|item| {
                        let content = item
                            .get("step")
                            .and_then(|v| v.as_str())
                            .map(str::trim)
                            .filter(|s| !s.is_empty())?
                            .to_string();
                        let status = item
                            .get("status")
                            .and_then(|v| v.as_str())
                            .map(|s| s.parse().unwrap_or(TodoStatus::Pending))
                            .unwrap_or(TodoStatus::Pending);

                        Some(TodoItem {
                            content,
                            status,
                            priority: TodoPriority::Medium,
                        })
                    })
                    .collect::<Vec<_>>(),
            )
        })
        .unwrap_or_default()
}

pub(super) fn is_running_rollout_event(event: &RolloutEvent) -> bool {
    if event.event_type == "event_msg"
        && event.payload.get("type").and_then(|v| v.as_str()) == Some("task_started")
    {
        return true;
    }

    event.event_type == "response_item"
        && event.payload.get("type").and_then(|v| v.as_str()) == Some("function_call")
}

// ── Status Inference ──────────────────────────────────────────────

/// Infer the status of a Codex rollout from its events.
///
/// - The latest relevant event wins
/// - `task_complete` → Done
/// - `turn_aborted` → Aborted
/// - Any event_msg with type containing "error" → Error
/// - `task_started` or `function_call` → Running
/// - Otherwise → Unknown
pub fn infer_rollout_status(events: &[RolloutEvent]) -> CodexInferredStatus {
    for event in events.iter().rev() {
        if event.event_type == "event_msg"
            && event.payload.get("type").and_then(|v| v.as_str()) == Some("turn_aborted")
        {
            return CodexInferredStatus::Aborted;
        }

        if event.event_type == "event_msg"
            && event
                .payload
                .get("type")
                .and_then(|v| v.as_str())
                .is_some_and(|t| t.contains("error"))
        {
            return CodexInferredStatus::Error;
        }

        if event.event_type == "event_msg"
            && event.payload.get("type").and_then(|v| v.as_str()) == Some("task_complete")
        {
            return CodexInferredStatus::Done;
        }

        if is_running_rollout_event(event) {
            return CodexInferredStatus::Running;
        }
    }

    CodexInferredStatus::Unknown
}
