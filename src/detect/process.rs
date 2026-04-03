use crate::detect::registry::AgentRegistry;
use crate::event::AgentEvent;
use crate::types::AgentId;
use crate::types::AgentTool;
use chrono::{DateTime, Local, NaiveDateTime};
use std::collections::HashSet;
use std::path::PathBuf;
use std::time::Duration;
use tokio::process::Command;
use tokio::sync::mpsc;
use tracing::{debug, warn};

#[derive(Debug, Clone)]
pub struct ProcessInfo {
    pub pid: u32,
    pub tool: AgentTool,
    pub started_at: Option<DateTime<Local>>,
    pub codex_thread_id: Option<String>,
    pub claude_resume_session_id: Option<String>,
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

    // Detect if lstart is present by checking for STARTED column.
    // lstart format: "Thu Mar 27 14:30:00 2026" (24 chars).
    let has_lstart = header.contains("STARTED");

    for line in lines {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        if has_lstart {
            // Format: "PID <24-char lstart> COMMAND"
            let mut parts = line.splitn(2, char::is_whitespace);
            let pid_str = match parts.next() {
                Some(s) => s.trim(),
                None => continue,
            };
            let rest = match parts.next() {
                Some(s) => s.trim_start(),
                None => continue,
            };
            let pid: u32 = match pid_str.parse() {
                Ok(p) => p,
                Err(_) => continue,
            };
            // `ps -o lstart` uses a fixed-width English-like representation on the
            // platforms we currently support. Keep the locale-sensitive assumption
            // isolated here so the rest of process detection can stay platform-agnostic.
            if rest.len() < 24 {
                continue;
            }
            let lstart_str = &rest[..24];
            let cmd = rest[24..].trim();
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
        } else {
            // Fallback: no lstart column, original format "PID COMMAND"
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

pub async fn get_process_cwd(pid: u32) -> Option<PathBuf> {
    // NOTE: On macOS, `lsof -p <pid> -Fn -d cwd` ignores the -p filter and
    // dumps every process's cwd. We must omit `-d cwd` and parse the file
    // descriptor type markers ourselves.
    let output = Command::new("lsof")
        .args(["-p", &pid.to_string(), "-Fn"])
        .output()
        .await
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_lsof_cwd(&stdout)
}

/// Resolve CWDs for a batch of PIDs using a single `lsof` invocation.
///
/// Returns a map of PID → PathBuf for every PID whose cwd could be resolved.
/// PIDs whose cwd cannot be determined are absent from the map.
pub async fn batch_get_cwds(pids: &[u32]) -> std::collections::HashMap<u32, PathBuf> {
    if pids.is_empty() {
        return std::collections::HashMap::new();
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
            return std::collections::HashMap::new();
        }
    };

    if !output.status.success() && output.stdout.is_empty() {
        return std::collections::HashMap::new();
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_lsof_cwds_multi(&stdout)
}

pub async fn scan_processes() -> Vec<(ProcessInfo, PathBuf)> {
    let output = match Command::new("ps")
        .args(["-eo", "pid,lstart,command"])
        .output()
        .await
    {
        Ok(o) => o,
        Err(e) => {
            warn!("Failed to run ps: {e}");
            return Vec::new();
        }
    };
    let stdout = String::from_utf8_lossy(&output.stdout);
    let procs = parse_ps_output(&stdout);

    // Resolve CWDs for all discovered processes in a single lsof call.
    let pids: Vec<u32> = procs.iter().map(|p| p.pid).collect();
    let mut cwd_map = batch_get_cwds(&pids).await;

    procs
        .into_iter()
        .filter_map(|proc| {
            if let Some(cwd) = cwd_map.remove(&proc.pid) {
                Some((proc, cwd))
            } else {
                debug!("Could not get CWD for pid {}", proc.pid);
                None
            }
        })
        .collect()
}

pub async fn run_process_scanner(
    tx: mpsc::Sender<AgentEvent>,
    poll_interval: Duration,
    registry: AgentRegistry,
) -> anyhow::Result<()> {
    let mut known_pids: HashSet<(AgentTool, u32)> = HashSet::new();
    loop {
        let found = scan_processes().await;
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
                    if let Some(thread_id) = proc.codex_thread_id.as_deref() {
                        registry.register_codex_thread(thread_id.to_string(), proc.pid);
                    }
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
        for (tool, pid) in exited {
            debug!("Process exited: {} pid={}", tool.as_str(), pid);
            registry.remove(pid);
            let _ = tx
                .send(AgentEvent::ProcessExited {
                    id: AgentId::new(tool, pid),
                    pid: Some(pid),
                })
                .await;
        }
        known_pids = current_pids;
        tokio::time::sleep(poll_interval).await;
    }
}
