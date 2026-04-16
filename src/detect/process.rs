use crate::detect::registry::RegistryHandle;
use crate::domain::agents::ids::AgentId;
use crate::domain::agents::ids::AgentTool;
use crate::domain::system::DetectorKind;
use crate::event::AgentEvent;
use chrono::{DateTime, Local, NaiveDateTime};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::Duration;
use tokio::process::Command;
use tokio::sync::mpsc;
use tracing::{debug, warn};

type CwdCache = HashMap<u32, PathBuf>;

#[derive(Debug, Clone)]
pub struct ProcessInfo {
    pub pid: u32,
    pub ppid: Option<u32>,
    pub tool: AgentTool,
    pub started_at: Option<DateTime<Local>>,
    pub codex_thread_id: Option<String>,
    pub claude_resume_session_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ScannerHealth {
    Healthy,
    Degraded(String),
}

struct ScanProcessesResult {
    processes: Vec<(ProcessInfo, PathBuf)>,
    health: ScannerHealth,
}

fn extract_codex_thread_id(cmd: &str) -> Option<String> {
    let tokens = cmd.split_whitespace().collect::<Vec<_>>();
    for window in tokens.windows(3) {
        if window[0]
            .rsplit('/')
            .next()
            .unwrap_or(window[0])
            .eq_ignore_ascii_case("codex")
            && window[1] == "resume"
        {
            let candidate = window[2];
            let valid = candidate.len() == 36
                && candidate
                    .chars()
                    .all(|ch| ch.is_ascii_hexdigit() || ch == '-');
            if valid {
                return Some(candidate.to_string());
            }
        }
    }

    None
}

pub fn extract_claude_resume_session(cmd: &str) -> Option<String> {
    let tokens = cmd.split_whitespace().collect::<Vec<_>>();
    for window in tokens.windows(3) {
        if window[0]
            .rsplit('/')
            .next()
            .unwrap_or(window[0])
            .eq_ignore_ascii_case("claude")
            && window[1] == "resume"
        {
            let candidate = window[2];
            if !candidate.is_empty() && !candidate.starts_with('-') {
                return Some(candidate.to_string());
            }
        }
    }

    None
}

pub fn parse_ps_output(output: &str) -> Vec<ProcessInfo> {
    let mut results = Vec::new();
    let mut lines = output.lines();
    let header = match lines.next() {
        Some(h) => h,
        None => return results,
    };

    // Detect format by checking for specific columns
    let has_start = header.contains("STARTED");
    let has_ppid = header.contains("PPID");

    for line in lines {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        if has_start && has_ppid {
            // Format: "PID PPID <24-char lstart> COMMAND"
            // Need to handle variable whitespace between columns
            let mut parts = line.split_whitespace();
            let pid_str = match parts.next() {
                Some(s) => s,
                None => continue,
            };
            let ppid_str = match parts.next() {
                Some(s) => s,
                None => continue,
            };
            let remaining: String = parts.collect::<Vec<_>>().join(" ");

            let pid: u32 = match pid_str.parse() {
                Ok(p) => p,
                Err(_) => continue,
            };
            let ppid: Option<u32> = ppid_str.parse().ok();
            // `ps -o lstart` uses a fixed-width English-like representation on the
            // platforms we currently support. Keep the locale-sensitive assumption
            // isolated here so the rest of process detection can stay platform-agnostic.
            if remaining.len() < 24 {
                continue;
            }
            let lstart_str = &remaining[..24];
            let cmd = remaining[24..].trim();
            let started_at =
                NaiveDateTime::parse_from_str(lstart_str.trim(), "%a %b %d %H:%M:%S %Y")
                    .or_else(|_| {
                        // macOS uses single-digit day with extra space: "Thu Mar  7 14:30:00 2026"
                        NaiveDateTime::parse_from_str(lstart_str.trim(), "%a %b  %d %H:%M:%S %Y")
                    })
                    .ok()
                    .map(|naive| {
                        naive
                            .and_local_timezone(Local)
                            .single()
                            .unwrap_or_else(Local::now)
                    });

            if let Some(tool) = detect_tool_from_command(cmd) {
                results.push(ProcessInfo {
                    pid,
                    ppid,
                    tool,
                    started_at,
                    codex_thread_id: (tool == AgentTool::Codex)
                        .then(|| extract_codex_thread_id(cmd))
                        .flatten(),
                    claude_resume_session_id: (tool == AgentTool::Claude)
                        .then(|| extract_claude_resume_session(cmd))
                        .flatten(),
                });
            }
        } else if has_ppid {
            // Format: "PID PPID COMMAND" (no STARTED column)
            let mut parts = line.splitn(3, char::is_whitespace);
            let pid_str = match parts.next() {
                Some(s) => s.trim(),
                None => continue,
            };
            let ppid_str = match parts.next() {
                Some(s) => s.trim(),
                None => continue,
            };
            let cmd = match parts.next() {
                Some(s) => s.trim(),
                None => continue,
            };
            let pid: u32 = match pid_str.parse() {
                Ok(p) => p,
                Err(_) => continue,
            };
            let ppid: Option<u32> = ppid_str.parse().ok();
            if let Some(tool) = detect_tool_from_command(cmd) {
                results.push(ProcessInfo {
                    pid,
                    ppid,
                    tool,
                    started_at: None,
                    codex_thread_id: (tool == AgentTool::Codex)
                        .then(|| extract_codex_thread_id(cmd))
                        .flatten(),
                    claude_resume_session_id: (tool == AgentTool::Claude)
                        .then(|| extract_claude_resume_session(cmd))
                        .flatten(),
                });
            }
        } else {
            // Original fallback: "PID COMMAND"
            let mut parts = line.splitn(2, char::is_whitespace);
            let pid_str = match parts.next() {
                Some(s) => s.trim(),
                None => continue,
            };
            let cmd = match parts.next() {
                Some(s) => s.trim(),
                None => continue,
            };
            let pid: u32 = match pid_str.parse() {
                Ok(p) => p,
                Err(_) => continue,
            };
            if let Some(tool) = detect_tool_from_command(cmd) {
                results.push(ProcessInfo {
                    pid,
                    ppid: None,
                    tool,
                    started_at: None,
                    codex_thread_id: (tool == AgentTool::Codex)
                        .then(|| extract_codex_thread_id(cmd))
                        .flatten(),
                    claude_resume_session_id: (tool == AgentTool::Claude)
                        .then(|| extract_claude_resume_session(cmd))
                        .flatten(),
                });
            }
        }
    }
    results
}

fn detect_tool_from_command(cmd: &str) -> Option<AgentTool> {
    // Check each token in the command for a matching binary name.
    // For commands like `node /Users/dev/.claude/local/claude`, the executable
    // is `node` but the script path ends with `claude`. We extract the filename
    // (after the last `/`) from each token and match exactly against known names.
    for token in cmd.split_whitespace() {
        let binary_name = token.rsplit('/').next().unwrap_or(token);
        let lower = binary_name.to_lowercase();
        match lower.as_str() {
            "opencode" => return Some(AgentTool::OpenCode),
            "claude" => return Some(AgentTool::Claude),
            "codex" => return Some(AgentTool::Codex),
            _ => {}
        }
    }
    None
}

/// Parse lsof -Fn output to extract the cwd path.
/// Looks for the `fcwd` file-descriptor type marker, then reads the next
/// `n`-prefixed line as the path.
#[cfg(test)]
pub fn parse_lsof_cwd(output: &str) -> Option<PathBuf> {
    let mut found_cwd = false;
    for line in output.lines() {
        if line == "fcwd" {
            found_cwd = true;
            continue;
        }
        if found_cwd {
            if let Some(path) = line.strip_prefix('n') {
                if path != "/" {
                    return Some(PathBuf::from(path));
                }
            }
            break; // cwd entry processed, stop
        }
    }
    None
}

/// Parse multi-PID lsof -Fn output into a map of PID → cwd path.
///
/// Each process section starts with a `p<PID>` line. Within each section,
/// `fcwd` marks the current-working-directory file descriptor and the
/// following `n<path>` line is the path.
pub fn parse_lsof_cwds_multi(output: &str) -> std::collections::HashMap<u32, PathBuf> {
    let mut map = std::collections::HashMap::new();
    let mut current_pid: Option<u32> = None;
    let mut found_cwd = false;

    for line in output.lines() {
        if let Some(rest) = line.strip_prefix('p') {
            if let Ok(pid) = rest.parse::<u32>() {
                current_pid = Some(pid);
                found_cwd = false;
                continue;
            }
        }
        if line == "fcwd" {
            found_cwd = true;
            continue;
        }
        if found_cwd {
            if let (Some(pid), Some(path)) = (current_pid, line.strip_prefix('n')) {
                if path != "/" {
                    map.insert(pid, PathBuf::from(path));
                }
            }
            found_cwd = false; // consume at most one `n` line per cwd fd
        }
    }
    map
}

/// Resolve CWDs for a batch of PIDs using a single `lsof` invocation.
///
/// Returns a map of PID → PathBuf for every PID whose cwd could be resolved.
/// PIDs whose cwd cannot be determined are absent from the map.
fn lsof_failure_message(error: &std::io::Error) -> String {
    if error.kind() == std::io::ErrorKind::NotFound {
        "Process scanner degraded: `lsof` is unavailable; install it to enable agent detection."
            .to_string()
    } else {
        format!("Process scanner degraded: failed to run `lsof`: {error}")
    }
}

fn ps_failure_message(error: &std::io::Error) -> String {
    if error.kind() == std::io::ErrorKind::NotFound {
        "Process scanner degraded: `ps` is unavailable; install it to enable agent detection."
            .to_string()
    } else {
        format!("Process scanner degraded: failed to run `ps`: {error}")
    }
}

pub async fn batch_get_cwds(
    pids: &[u32],
) -> (std::collections::HashMap<u32, PathBuf>, Option<String>) {
    if pids.is_empty() {
        return (std::collections::HashMap::new(), None);
    }
    // Build comma-separated PID list: "1234,5678,..."
    let pid_arg = pids
        .iter()
        .map(|p| p.to_string())
        .collect::<Vec<_>>()
        .join(",");

    let output = match Command::new("lsof")
        .args(["-p", &pid_arg, "-Fn"])
        .output()
        .await
    {
        Ok(o) => o,
        Err(e) => {
            debug!("lsof batch call failed: {e}");
            return (
                std::collections::HashMap::new(),
                Some(lsof_failure_message(&e)),
            );
        }
    };

    if !output.status.success() && output.stdout.is_empty() {
        return (
            std::collections::HashMap::new(),
            Some("Process scanner degraded: `lsof` did not return cwd data.".to_string()),
        );
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    (parse_lsof_cwds_multi(&stdout), None)
}

async fn scan_processes(cwd_cache: &mut CwdCache) -> ScanProcessesResult {
    let output = match Command::new("ps")
        .args(["-eo", "pid,ppid,lstart,command"])
        .output()
        .await
    {
        Ok(o) => o,
        Err(e) => {
            warn!("Failed to run ps: {e}");
            return ScanProcessesResult {
                processes: Vec::new(),
                health: ScannerHealth::Degraded(ps_failure_message(&e)),
            };
        }
    };
    let stdout = String::from_utf8_lossy(&output.stdout);
    let procs = parse_ps_output(&stdout);

    // Separate PIDs: those we have cached vs those we need to look up
    let new_pids: Vec<u32> = procs
        .iter()
        .filter(|p| !cwd_cache.contains_key(&p.pid))
        .map(|p| p.pid)
        .collect();
    let cached_procs_data: Vec<(u32, PathBuf)> = procs
        .iter()
        .filter_map(|p| cwd_cache.get(&p.pid).map(|cwd| (p.pid, cwd.clone())))
        .collect();

    // Start with cached CWDs - we now have (pid, cwd) pairs, need to rejoin with ProcessInfo
    let mut pid_cwd: std::collections::HashMap<u32, PathBuf> =
        cached_procs_data.into_iter().collect();

    // Only call lsof for PIDs we haven't cached yet
    if !new_pids.is_empty() {
        let (new_cwds, lsof_issue) = batch_get_cwds(&new_pids).await;

        // Update cache and results
        for (pid, cwd) in &new_cwds {
            cwd_cache.insert(*pid, cwd.clone());
        }

        pid_cwd.extend(new_cwds);

        if let Some(issue) = lsof_issue {
            return ScanProcessesResult {
                processes: Vec::new(),
                health: ScannerHealth::Degraded(issue),
            };
        }
    }

    // Filter out processes without CWD
    let mut pid_info: std::collections::HashMap<u32, (AgentTool, Option<u32>)> =
        std::collections::HashMap::new();

    let procs_with_cwd: Vec<(ProcessInfo, PathBuf)> = procs
        .into_iter()
        .filter_map(|proc| {
            pid_info.insert(proc.pid, (proc.tool, proc.ppid));
            pid_cwd.remove(&proc.pid).map(|cwd| (proc, cwd))
        })
        .collect();

    // Get all CWDs for parent lookup (including from cache)
    let cwd_map: &HashMap<u32, PathBuf> = cwd_cache;

    // Filter out child processes that have the same tool as their parent
    // This prevents duplicate detection when a tool spawns a child process
    // with the same command (like opencode's node wrapper)
    let results: Vec<(ProcessInfo, PathBuf)> = procs_with_cwd
        .into_iter()
        .filter_map(|(proc, cwd)| {
            // Check if this process has a child with same tool and same CWD
            // If so, skip this parent (keep the child which is the actual binary)
            // This handles cases like opencode where node wrapper spawns the actual binary
            let has_same_tool_child_with_same_cwd = proc.ppid.is_some() && {
                // Check if this process's parent also has same tool
                proc.ppid
                    .and_then(|ppid| pid_info.get(&ppid))
                    .map(|(parent_tool, _)| *parent_tool == proc.tool)
                    .unwrap_or(false)
            };

            if has_same_tool_child_with_same_cwd {
                if let Some(ppid) = proc.ppid {
                    if let Some(parent_cwd) = cwd_map.get(&ppid) {
                        if parent_cwd == &cwd {
                            debug!(
                                "Skipping parent process {} (child has same tool/CWD)",
                                proc.pid
                            );
                            return None;
                        }
                    }
                }
            }

            Some((proc, cwd))
        })
        .collect();

    ScanProcessesResult {
        processes: results,
        health: ScannerHealth::Healthy,
    }
}

pub async fn run_process_scanner(
    tx: mpsc::Sender<AgentEvent>,
    poll_interval: Duration,
    registry: RegistryHandle,
) -> anyhow::Result<()> {
    let mut known_pids: HashSet<(AgentTool, u32)> = HashSet::new();
    let mut last_health = ScannerHealth::Healthy;
    let mut cwd_cache = CwdCache::new();
    loop {
        let scan = scan_processes(&mut cwd_cache).await;
        if scan.health != last_health {
            match &scan.health {
                ScannerHealth::Healthy => {
                    let _ = tx
                        .send(AgentEvent::DetectorHealthChanged {
                            detector: DetectorKind::ProcessScanner,
                            healthy: true,
                            message: "process scanner healthy".to_string(),
                        })
                        .await;
                }
                ScannerHealth::Degraded(message) => {
                    let _ = tx
                        .send(AgentEvent::DetectorHealthChanged {
                            detector: DetectorKind::ProcessScanner,
                            healthy: false,
                            message: message.clone(),
                        })
                        .await;
                }
            }
            last_health = scan.health.clone();
        }

        if scan.health != ScannerHealth::Healthy {
            tokio::time::sleep(poll_interval).await;
            continue;
        }

        let found = scan.processes;
        let mut current_pids: HashSet<(AgentTool, u32)> = HashSet::new();
        for (proc, cwd) in &found {
            let key = (proc.tool, proc.pid);
            current_pids.insert(key);
            if !known_pids.contains(&key) {
                debug!("Discovered {} pid={}", proc.tool.as_str(), proc.pid);
                registry.register(proc.tool, cwd.clone(), proc.pid);
                if proc.tool == AgentTool::Claude {
                    registry.register_claude_launch(
                        proc.pid,
                        proc.started_at,
                        proc.claude_resume_session_id.clone(),
                    );
                }
                if proc.tool == AgentTool::Codex {
                    registry.register_codex_launch(proc.pid, proc.started_at);
                    if let Some(thread_id) = proc.codex_thread_id.as_deref() {
                        registry.register_codex_thread(thread_id.to_string(), proc.pid);
                    }
                }
                if proc.tool == AgentTool::OpenCode {
                    registry.register_opencode_launch(proc.pid, proc.started_at);
                }
                let _ = tx
                    .send(AgentEvent::Discovered {
                        id: AgentId::new(proc.tool, proc.pid),
                        pid: Some(proc.pid),
                        cwd: cwd.clone(),
                        started_at: proc.started_at,
                    })
                    .await;
            }
        }
        let exited: Vec<_> = known_pids.difference(&current_pids).cloned().collect();
        for (tool, pid) in &exited {
            debug!("Process exited: {} pid={}", tool.as_str(), pid);
            registry.remove(*pid);
            cwd_cache.remove(pid);
            let _ = tx
                .send(AgentEvent::ProcessExited {
                    id: AgentId::new(*tool, *pid),
                    pid: Some(*pid),
                })
                .await;
        }
        known_pids = current_pids;
        tokio::time::sleep(poll_interval).await;
    }
}

#[cfg(test)]
#[path = "process_tests.rs"]
mod tests;
