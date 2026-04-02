use ai_overview::detect::registry::AgentRegistry;
use ai_overview::types::AgentTool;
use std::path::PathBuf;

#[test]
fn test_codex_registry_preserves_multiple_pids_per_cwd() {
    let registry = AgentRegistry::new();
    let cwd = PathBuf::from("/tmp/project");

    registry.register(AgentTool::Codex, cwd.clone(), 101);
    registry.register(AgentTool::Codex, cwd.clone(), 202);

    assert_eq!(registry.find_pid(AgentTool::Codex, &cwd), Some(101));
    assert_eq!(registry.find_pids(AgentTool::Codex, &cwd), vec![101, 202]);

    let agents = registry.agents_for_tool(AgentTool::Codex);
    assert_eq!(agents, vec![(cwd, 101), (PathBuf::from("/tmp/project"), 202)]);
}

#[test]
fn test_non_codex_registry_still_overwrites_pid_for_same_cwd() {
    let registry = AgentRegistry::new();
    let cwd = PathBuf::from("/tmp/project");

    registry.register(AgentTool::Claude, cwd.clone(), 303);
    registry.register(AgentTool::Claude, cwd.clone(), 404);

    assert_eq!(registry.find_pid(AgentTool::Claude, &cwd), Some(404));
    assert_eq!(registry.find_pids(AgentTool::Claude, &cwd), vec![404]);
    assert_eq!(registry.agents_for_tool(AgentTool::Claude), vec![(cwd, 404)]);
}

#[test]
fn test_codex_remove_only_drops_matching_pid() {
    let registry = AgentRegistry::new();
    let cwd = PathBuf::from("/tmp/project");

    registry.register(AgentTool::Codex, cwd.clone(), 101);
    registry.register(AgentTool::Codex, cwd.clone(), 202);
    registry.remove(101);

    assert_eq!(registry.find_pid(AgentTool::Codex, &cwd), Some(202));
    assert_eq!(registry.find_pids(AgentTool::Codex, &cwd), vec![202]);
    assert_eq!(registry.agents_for_tool(AgentTool::Codex), vec![(cwd, 202)]);
}
