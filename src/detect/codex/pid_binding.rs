use std::collections::HashMap;
use std::path::Path;

use chrono::Utc;

use crate::detect::registry::RegistrySnapshot;
use nexus_domain::agents::ids::AgentTool;

use super::sqlite::parse_source_field;
use super::types::{CodexThread, CodexThreadClassification, SourceInfo};
use super::{CONSERVATIVE_PID_BINDING_WINDOW_SECS, SAME_CWD_MULTI_BIND_MAX_SKEW_SECS};

// ── PID Binding Helpers ───────────────────────────────────────────────

pub(crate) fn codex_pid_for_thread(registry: &RegistrySnapshot, cwd: &Path) -> Option<u32> {
    match registry.find_pids(AgentTool::Codex, cwd).as_slice() {
        [pid] => Some(*pid),
        _ => None,
    }
}

pub(super) fn codex_pid_for_exact_thread(
    registry: &RegistrySnapshot,
    thread_id: &str,
) -> Option<u32> {
    registry.find_codex_pid_for_thread(thread_id)
}

pub(super) fn codex_launch_started_at(registry: &RegistrySnapshot, pid: u32) -> Option<i64> {
    registry
        .find_codex_launch(pid)
        .and_then(|launch| launch.started_at.map(|started_at| started_at.timestamp()))
}

pub(super) fn thread_is_recent_enough_for_pid_binding(thread: &CodexThread) -> bool {
    let thread_timestamp = thread.created_at.max(thread.updated_at);
    if thread_timestamp <= 0 {
        return false;
    }

    let now = Utc::now().timestamp();
    let age = now.saturating_sub(thread_timestamp);
    age <= CONSERVATIVE_PID_BINDING_WINDOW_SECS
}

pub(super) fn classify_codex_thread(
    thread: &CodexThread,
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

/// Bind PIDs to threads using exact matches, same-CWD heuristics, and time-skew matching.
///
/// This function encapsulates all PID binding logic:
/// - Exact bindings from registry (thread_id → PID)
/// - Same-CWD single thread/process bindings
/// - Multi-bind time-skew matching for multiple threads/PIDs in the same CWD
pub(super) fn bind_pids_to_threads(
    classified_threads: &[(CodexThread, CodexThreadClassification)],
    registry: &RegistrySnapshot,
) -> HashMap<String, u32> {
    let mut fallback_pid_by_thread_id: HashMap<String, u32> = HashMap::new();
    let mut exact_bound_pids = Vec::new();

    // Step 1: Exact bindings from registry
    for (thread, classification) in classified_threads {
        if matches!(classification, CodexThreadClassification::TopLevel) {
            if let Some(pid) = codex_pid_for_exact_thread(registry, &thread.id) {
                fallback_pid_by_thread_id.insert(thread.id.clone(), pid);
                exact_bound_pids.push(pid);
            }
        }
    }

    // Step 2: Collect bindable threads by CWD (excluding already-bound threads)
    let mut bindable_threads_by_cwd: HashMap<String, Vec<&CodexThread>> = HashMap::new();
    for (thread, classification) in classified_threads {
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

    // Step 3: Bind threads to PIDs by CWD
    for (cwd, candidates) in bindable_threads_by_cwd {
        let available_pids = registry
            .find_pids(AgentTool::Codex, Path::new(&cwd))
            .into_iter()
            .filter(|pid| !exact_bound_pids.contains(pid))
            .collect::<Vec<_>>();

        if available_pids.is_empty() {
            continue;
        }

        // Simple case: 1 thread + 1 PID
        if candidates.len() == 1 && available_pids.len() == 1 {
            fallback_pid_by_thread_id.insert(candidates[0].id.clone(), available_pids[0]);
            continue;
        }

        // Multi-bind case: N threads + N PIDs (must match count)
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

    fallback_pid_by_thread_id
}
