use crate::detect::registry::AgentRegistry;
use crate::event::AgentEvent;
use crate::types::AgentTool;
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
}

pub fn parse_ps_output(output: &str) -> Vec<ProcessInfo> {
    let mut results = Vec::new();
    for line in output.lines().skip(1) {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
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
            results.push(ProcessInfo { pid, tool });
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

pub async fn scan_processes() -> Vec<(ProcessInfo, PathBuf)> {
    let output = match Command::new("ps")
        .args(["-eo", "pid,command"])
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
    let mut results = Vec::new();
    for proc in procs {
        if let Some(cwd) = get_process_cwd(proc.pid).await {
            results.push((proc, cwd));
        } else {
            debug!("Could not get CWD for pid {}", proc.pid);
        }
    }
    results
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
                let _ = tx
                    .send(AgentEvent::Discovered {
                        tool: proc.tool,
                        pid: proc.pid,
                        cwd: cwd.clone(),
                    })
                    .await;
            }
        }
        let exited: Vec<_> = known_pids.difference(&current_pids).cloned().collect();
        for (tool, pid) in exited {
            debug!("Process exited: {} pid={}", tool.as_str(), pid);
            registry.remove(pid);
            let _ = tx.send(AgentEvent::ProcessExited { pid, tool }).await;
        }
        known_pids = current_pids;
        tokio::time::sleep(poll_interval).await;
    }
}
