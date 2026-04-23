use super::{
    extract_claude_resume_session, parse_lsof_cwd, parse_ps_output as parse_ps_output_impl,
};
use nexus_domain::agents::AgentTool;
use std::path::PathBuf;

#[test]
fn parse_ps_output() {
    let output = include_str!("../../tests/fixtures/ps_output.txt");
    let procs = super::parse_ps_output(output);
    assert_eq!(procs.len(), 4);

    let opencode = procs.iter().find(|process| process.pid == 12345).unwrap();
    assert_eq!(opencode.tool, AgentTool::OpenCode);

    let claude_one = procs.iter().find(|process| process.pid == 23456).unwrap();
    assert_eq!(claude_one.tool, AgentTool::Claude);

    let codex = procs.iter().find(|process| process.pid == 34567).unwrap();
    assert_eq!(codex.tool, AgentTool::Codex);

    let claude_two = procs.iter().find(|process| process.pid == 99999).unwrap();
    assert_eq!(claude_two.tool, AgentTool::Claude);
}

#[test]
fn test_parse_ps_output_ignores_non_agents() {
    let output = "  PID COMMAND\n45678 /bin/bash\n";
    let procs = parse_ps_output_impl(output);
    assert!(procs.is_empty());
}

#[test]
fn test_parse_ps_output_handles_empty() {
    let procs = parse_ps_output_impl("");
    assert!(procs.is_empty());
}

#[test]
fn test_no_false_positive_on_username_path() {
    let output = "  PID COMMAND\n11111 /Users/claudette/bin/python\n";
    let procs = parse_ps_output_impl(output);
    assert!(
        procs.is_empty(),
        "path containing 'claude' as a directory name should not match"
    );
}

#[test]
fn test_no_false_positive_on_lib_path() {
    let output = "  PID COMMAND\n22222 /home/user/codex-lib/main\n";
    let procs = parse_ps_output_impl(output);
    assert!(
        procs.is_empty(),
        "path containing 'codex' as a directory prefix should not match"
    );
}

#[test]
fn test_matches_binary_with_flags() {
    let output = "  PID COMMAND\n33333 /usr/local/bin/claude --flag\n";
    let procs = parse_ps_output_impl(output);
    assert_eq!(procs.len(), 1);
    assert_eq!(procs[0].pid, 33333);
    assert_eq!(procs[0].tool, AgentTool::Claude);
}

#[test]
fn test_matches_bare_command() {
    let output = "  PID COMMAND\n44444 opencode\n";
    let procs = parse_ps_output_impl(output);
    assert_eq!(procs.len(), 1);
    assert_eq!(procs[0].pid, 44444);
    assert_eq!(procs[0].tool, AgentTool::OpenCode);
}

#[test]
fn test_matches_codex_with_args() {
    let output = "  PID COMMAND\n55555 codex --model gpt-4\n";
    let procs = parse_ps_output_impl(output);
    assert_eq!(procs.len(), 1);
    assert_eq!(procs[0].pid, 55555);
    assert_eq!(procs[0].tool, AgentTool::Codex);
    assert_eq!(procs[0].codex_thread_id, None);
}

#[test]
fn test_parse_ps_output_extracts_codex_resume_thread_id() {
    let output =
            "  PID COMMAND\n15642 codex resume 019d4ffb-ca93-7c83-9cb5-72695ed739e0 --dangerously-bypass-approvals-and-sandbox\n";
    let procs = parse_ps_output_impl(output);
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
    let output = "p100\nfcwd\nn/\nftxt\nn/usr/bin/foo\n";
    assert_eq!(parse_lsof_cwd(output), None);
}

#[test]
fn test_parse_lsof_cwd_no_cwd_entry() {
    let output = "p200\nftxt\nn/usr/bin/foo\n";
    assert_eq!(parse_lsof_cwd(output), None);
}

#[test]
fn test_parse_lsof_cwd_ignores_other_fd_paths() {
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
    let output = include_str!("../../tests/fixtures/ps_output.txt");
    let procs = parse_ps_output_impl(output);
    let first = &procs[0];
    assert_eq!(first.pid, 12345);
    assert_eq!(first.tool, AgentTool::OpenCode);
    let started = first.started_at.expect("should have start time");
    // ps outputs local time; we store as UTC then convert back for display comparison
    let local = started.with_timezone(&chrono::Local);
    assert_eq!(
        local.format("%Y-%m-%d %H:%M:%S").to_string(),
        "2026-03-27 14:30:00"
    );
}

#[test]
fn test_parse_ps_output_all_have_start_times() {
    let output = include_str!("../../tests/fixtures/ps_output.txt");
    let procs = parse_ps_output_impl(output);
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
    let procs = parse_ps_output_impl(output);
    assert_eq!(procs.len(), 1);
    assert!(procs[0].started_at.is_none());
}
