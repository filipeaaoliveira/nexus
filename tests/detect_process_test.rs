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
