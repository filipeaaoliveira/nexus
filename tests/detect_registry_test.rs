use chrono::Local;
use nexus::detect::registry::AgentRegistry;
use nexus::types::AgentTool;
use std::path::PathBuf;

#[test]
fn test_codex_registry_returns_none_for_ambiguous_cwd() {
    let registry = AgentRegistry::new();
    let cwd = PathBuf::from("/tmp/project");

    registry.register(AgentTool::Codex, cwd.clone(), 101);
    registry.register(AgentTool::Codex, cwd.clone(), 202);

    assert_eq!(registry.find_pid(AgentTool::Codex, &cwd), None);
    assert_eq!(registry.find_pids(AgentTool::Codex, &cwd), vec![101, 202]);

    let agents = registry.agents_for_tool(AgentTool::Codex);
    assert_eq!(
        agents,
        vec![(cwd, 101), (PathBuf::from("/tmp/project"), 202)]
    );
}

#[test]
fn test_codex_registry_returns_pid_for_single_cwd_candidate() {
    let registry = AgentRegistry::new();
    let cwd = PathBuf::from("/tmp/project");

    registry.register(AgentTool::Codex, cwd.clone(), 101);

    assert_eq!(registry.find_pid(AgentTool::Codex, &cwd), Some(101));
    assert_eq!(registry.find_pids(AgentTool::Codex, &cwd), vec![101]);
    assert_eq!(registry.agents_for_tool(AgentTool::Codex), vec![(cwd, 101)]);
}

#[test]
fn test_non_codex_registry_still_overwrites_pid_for_same_cwd() {
    let registry = AgentRegistry::new();
    let cwd = PathBuf::from("/tmp/project");

    registry.register(AgentTool::Claude, cwd.clone(), 303);
    registry.register(AgentTool::Claude, cwd.clone(), 404);

    assert_eq!(registry.find_pid(AgentTool::Claude, &cwd), Some(404));
    assert_eq!(registry.find_pids(AgentTool::Claude, &cwd), vec![404]);
    assert_eq!(
        registry.agents_for_tool(AgentTool::Claude),
        vec![(cwd, 404)]
    );
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

#[test]
fn test_codex_registry_tracks_exact_thread_pid_binding() {
    let registry = AgentRegistry::new();
    let cwd = PathBuf::from("/tmp/project");

    registry.register(AgentTool::Codex, cwd, 15642);
    registry.register_codex_thread("019d4ffb-ca93-7c83-9cb5-72695ed739e0".to_string(), 15642);

    assert_eq!(
        registry.find_codex_pid_for_thread("019d4ffb-ca93-7c83-9cb5-72695ed739e0"),
        Some(15642)
    );

    registry.remove(15642);

    assert_eq!(
        registry.find_codex_pid_for_thread("019d4ffb-ca93-7c83-9cb5-72695ed739e0"),
        None
    );
}

#[test]
fn test_registry_tracks_claude_launch_metadata() {
    let registry = AgentRegistry::new();
    let started_at = Local::now();

    registry.register_claude_launch(4242, Some(started_at), Some("session-123".to_string()));

    let info = registry.find_claude_launch(4242).expect("launch info");
    assert_eq!(info.started_at, Some(started_at));
    assert_eq!(info.resume_session_id.as_deref(), Some("session-123"));

    registry.remove(4242);
    assert!(registry.find_claude_launch(4242).is_none());
}
