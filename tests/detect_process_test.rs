use ai_overview::detect::process::parse_ps_output;
use ai_overview::types::AgentTool;

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
}
