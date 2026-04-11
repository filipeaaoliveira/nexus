use nexus::detect::process::{extract_claude_resume_session, parse_lsof_cwd, parse_ps_output};
use nexus::types::AgentTool;
use std::path::PathBuf;

#[test]
fn test_parse_ps_output_finds_agents() {
    let output = include_str!("fixtures/ps_output.txt");
    let procs = parse_ps_output(output);
    assert_eq!(procs.len(), 4);
    let opencode = procs.iter().find(|p| p.pid == 12345).unwrap();
    assert_eq!(opencode.tool, AgentTool::OpenCode);
    let claude1 = procs.iter().find(|p| p.pid == 23456).unwrap();
    assert_eq!(claude1.tool, AgentTool::Claude);
    let codex = procs.iter().find(|p| p.pid == 34567).unwrap();
    assert_eq!(codex.tool, AgentTool::Codex);
    let claude2 = procs.iter().find(|p| p.pid == 99999).unwrap();
    assert_eq!(claude2.tool, AgentTool::Claude);
}

#[test]
fn test_parse_ps_output_ignores_non_agents() {
    let output = "  PID COMMAND\n45678 /bin/bash\n";
    let procs = parse_ps_output(output);
    assert!(procs.is_empty());
}

#[test]
fn test_parse_ps_output_handles_empty() {
    let procs = parse_ps_output("");
    assert!(procs.is_empty());
}

#[test]
fn test_no_false_positive_on_username_path() {
    // /Users/claudette/bin/python should NOT match as Claude
    let output = "  PID COMMAND\n11111 /Users/claudette/bin/python\n";
    let procs = parse_ps_output(output);
    assert!(
        procs.is_empty(),
        "path containing 'claude' as a directory name should not match"
    );
}

#[test]
fn test_no_false_positive_on_lib_path() {
    // /home/user/codex-lib/main should NOT match as Codex
    let output = "  PID COMMAND\n22222 /home/user/codex-lib/main\n";
    let procs = parse_ps_output(output);
    assert!(
        procs.is_empty(),
        "path containing 'codex' as a directory prefix should not match"
    );
}

#[test]
fn test_matches_binary_with_flags() {
    // /usr/local/bin/claude --flag should match as Claude
    let output = "  PID COMMAND\n33333 /usr/local/bin/claude --flag\n";
    let procs = parse_ps_output(output);
    assert_eq!(procs.len(), 1);
    assert_eq!(procs[0].pid, 33333);
    assert_eq!(procs[0].tool, AgentTool::Claude);
}

#[test]
fn test_matches_bare_command() {
    // bare `opencode` command should match
    let output = "  PID COMMAND\n44444 opencode\n";
    let procs = parse_ps_output(output);
    assert_eq!(procs.len(), 1);
    assert_eq!(procs[0].pid, 44444);
    assert_eq!(procs[0].tool, AgentTool::OpenCode);
}

#[test]
fn test_matches_codex_with_args() {
    let output = "  PID COMMAND\n55555 codex --model gpt-4\n";
    let procs = parse_ps_output(output);
    assert_eq!(procs.len(), 1);
    assert_eq!(procs[0].pid, 55555);
    assert_eq!(procs[0].tool, AgentTool::Codex);
    assert_eq!(procs[0].codex_thread_id, None);
}

#[test]
fn test_parse_ps_output_extracts_codex_resume_thread_id() {
    let output =
        "  PID COMMAND\n15642 codex resume 019d4ffb-ca93-7c83-9cb5-72695ed739e0 --dangerously-bypass-approvals-and-sandbox\n";
    let procs = parse_ps_output(output);
    assert_eq!(procs.len(), 1);
    assert_eq!(procs[0].pid, 15642);
    assert_eq!(procs[0].tool, AgentTool::Codex);
    assert_eq!(
        procs[0].codex_thread_id.as_deref(),
        Some("019d4ffb-ca93-7c83-9cb5-72695ed739e0")
    );
}

#[test]
fn test_extract_claude_resume_session_from_command_line() {
    let cmd = "/Users/dev/.npm/_npx/node /Users/dev/.claude/local/claude resume session-123 --dangerously-bypass-approvals-and-sandbox";
    assert_eq!(
        extract_claude_resume_session(cmd).as_deref(),
        Some("session-123")
    );
}

#[test]
fn test_extract_claude_resume_session_ignores_fresh_launches() {
    let cmd = "/Users/dev/.npm/_npx/node /Users/dev/.claude/local/claude --dangerously-bypass-approvals-and-sandbox";
    assert_eq!(extract_claude_resume_session(cmd), None);
}

#[test]
fn test_parse_lsof_cwd_extracts_correct_path() {
    // Real lsof -p <pid> -Fn output (without -d cwd)
    let output = "p53985\n\
                  fcwd\n\
                  n/Users/filipe/Documents/lab/ai-overview\n\
                  ftxt\n\
                  n/opt/homebrew/Cellar/opencode/1.3.9/bin/opencode\n\
                  ftxt\n\
                  n/usr/lib/dyld\n";
    assert_eq!(
        parse_lsof_cwd(output),
        Some(PathBuf::from("/Users/filipe/Documents/lab/ai-overview"))
    );
}

#[test]
fn test_parse_lsof_cwd_skips_root() {
    // If cwd is "/", return None
    let output = "p100\nfcwd\nn/\nftxt\nn/usr/bin/foo\n";
    assert_eq!(parse_lsof_cwd(output), None);
}

#[test]
fn test_parse_lsof_cwd_no_cwd_entry() {
    // No fcwd line at all
    let output = "p200\nftxt\nn/usr/bin/foo\n";
    assert_eq!(parse_lsof_cwd(output), None);
}

#[test]
fn test_parse_lsof_cwd_ignores_other_fd_paths() {
    // Should NOT return paths from ftxt or other fd types — only the one after fcwd
    let output = "p300\n\
                  ftxt\n\
                  n/Users/someone/Library/Containers/com.apple.KerberosMenuExtra/Data\n\
                  fcwd\n\
                  n/Users/filipe/projects/myapp\n\
                  ftxt\n\
                  n/opt/homebrew/bin/node\n";
    assert_eq!(
        parse_lsof_cwd(output),
        Some(PathBuf::from("/Users/filipe/projects/myapp"))
    );
}

#[test]
fn test_parse_lsof_cwd_empty_output() {
    assert_eq!(parse_lsof_cwd(""), None);
}

#[test]
fn test_parse_ps_output_extracts_start_time() {
    let output = include_str!("fixtures/ps_output.txt");
    let procs = parse_ps_output(output);
    // First process: opencode, started Thu Mar 27 14:30:00 2026
    let first = &procs[0];
    assert_eq!(first.pid, 12345);
    assert_eq!(first.tool, AgentTool::OpenCode);
    let started = first.started_at.expect("should have start time");
    assert_eq!(
        started.format("%Y-%m-%d %H:%M:%S").to_string(),
        "2026-03-27 14:30:00"
    );
}

#[test]
fn test_parse_ps_output_all_have_start_times() {
    let output = include_str!("fixtures/ps_output.txt");
    let procs = parse_ps_output(output);
    for proc in &procs {
        assert!(
            proc.started_at.is_some(),
            "pid {} should have start time",
            proc.pid
        );
    }
}

#[test]
fn test_parse_ps_output_no_lstart_fallback() {
    let output = "  PID COMMAND\n12345 /usr/local/bin/opencode\n";
    let procs = parse_ps_output(output);
    assert_eq!(procs.len(), 1);
    assert!(procs[0].started_at.is_none());
}
