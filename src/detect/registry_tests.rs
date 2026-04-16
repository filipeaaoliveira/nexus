use super::*;
use chrono::Local;
use std::path::PathBuf;

#[tokio::test]
async fn test_opencode_launch_register_and_find() {
    let handle = RegistryHandle::spawn();
    let now = Local::now();
    handle.register_opencode_launch(99, Some(now));
    let info = handle
        .find_opencode_launch(99)
        .await
        .expect("should find launch info");
    assert_eq!(info.started_at, Some(now));
}

#[tokio::test]
async fn test_opencode_launch_removed_on_remove() {
    let handle = RegistryHandle::spawn();
    let now = Local::now();
    handle.register_opencode_launch(99, Some(now));
    // Ensure registration is processed before remove
    let _ = handle.find_opencode_launch(99).await;
    handle.remove(99);
    // Ensure remove is processed
    assert!(handle.find_opencode_launch(99).await.is_none());
}

#[tokio::test]
async fn test_opencode_launch_none_started_at() {
    let handle = RegistryHandle::spawn();
    handle.register_opencode_launch(100, None);
    let info = handle.find_opencode_launch(100).await.expect("should find");
    assert!(info.started_at.is_none());
}

#[tokio::test]
async fn test_codex_registry_returns_none_for_ambiguous_cwd() {
    let handle = RegistryHandle::spawn();
    let cwd = PathBuf::from("/tmp/project");

    handle.register(AgentTool::Codex, cwd.clone(), 101);
    handle.register(AgentTool::Codex, cwd.clone(), 202);

    assert_eq!(handle.find_pid(AgentTool::Codex, &cwd).await, None);
    assert_eq!(
        handle.find_pids(AgentTool::Codex, &cwd).await,
        vec![101, 202]
    );
    assert_eq!(
        handle.agents_for_tool(AgentTool::Codex).await,
        vec![(cwd, 101), (PathBuf::from("/tmp/project"), 202)]
    );
}

#[tokio::test]
async fn test_codex_registry_returns_pid_for_single_cwd_candidate() {
    let handle = RegistryHandle::spawn();
    let cwd = PathBuf::from("/tmp/project");

    handle.register(AgentTool::Codex, cwd.clone(), 101);

    assert_eq!(handle.find_pid(AgentTool::Codex, &cwd).await, Some(101));
    assert_eq!(handle.find_pids(AgentTool::Codex, &cwd).await, vec![101]);
    assert_eq!(
        handle.agents_for_tool(AgentTool::Codex).await,
        vec![(cwd, 101)]
    );
}

#[tokio::test]
async fn test_non_codex_registry_still_overwrites_pid_for_same_cwd() {
    let handle = RegistryHandle::spawn();
    let cwd = PathBuf::from("/tmp/project");

    handle.register(AgentTool::Claude, cwd.clone(), 303);
    handle.register(AgentTool::Claude, cwd.clone(), 404);

    assert_eq!(handle.find_pid(AgentTool::Claude, &cwd).await, Some(404));
    assert_eq!(handle.find_pids(AgentTool::Claude, &cwd).await, vec![404]);
    assert_eq!(
        handle.agents_for_tool(AgentTool::Claude).await,
        vec![(cwd, 404)]
    );
}

#[tokio::test]
async fn test_codex_remove_only_drops_matching_pid() {
    let handle = RegistryHandle::spawn();
    let cwd = PathBuf::from("/tmp/project");

    handle.register(AgentTool::Codex, cwd.clone(), 101);
    handle.register(AgentTool::Codex, cwd.clone(), 202);
    // Ensure registrations are processed
    let _ = handle.find_pids(AgentTool::Codex, &cwd).await;
    handle.remove(101);

    assert_eq!(handle.find_pid(AgentTool::Codex, &cwd).await, Some(202));
    assert_eq!(handle.find_pids(AgentTool::Codex, &cwd).await, vec![202]);
    assert_eq!(
        handle.agents_for_tool(AgentTool::Codex).await,
        vec![(cwd, 202)]
    );
}

#[tokio::test]
async fn test_codex_registry_tracks_exact_thread_pid_binding() {
    let handle = RegistryHandle::spawn();
    let cwd = PathBuf::from("/tmp/project");

    handle.register(AgentTool::Codex, cwd, 15642);
    handle.register_codex_thread("019d4ffb-ca93-7c83-9cb5-72695ed739e0".to_string(), 15642);

    assert_eq!(
        handle
            .find_codex_pid_for_thread("019d4ffb-ca93-7c83-9cb5-72695ed739e0")
            .await,
        Some(15642)
    );

    handle.remove(15642);

    assert_eq!(
        handle
            .find_codex_pid_for_thread("019d4ffb-ca93-7c83-9cb5-72695ed739e0")
            .await,
        None
    );
}

#[tokio::test]
async fn register_codex_launch() {
    let handle = RegistryHandle::spawn();
    let started_at = Local::now();

    handle.register_codex_launch(4242, Some(started_at));

    let info = handle.find_codex_launch(4242).await.expect("launch info");
    assert_eq!(info.started_at, Some(started_at));

    handle.remove(4242);
    assert!(handle.find_codex_launch(4242).await.is_none());
}

#[tokio::test]
async fn test_registry_tracks_claude_launch_metadata() {
    let handle = RegistryHandle::spawn();
    let started_at = Local::now();

    handle.register_claude_launch(4242, Some(started_at), Some("session-123".to_string()));

    let info = handle.find_claude_launch(4242).await.expect("launch info");
    assert_eq!(info.started_at, Some(started_at));
    assert_eq!(info.resume_session_id.as_deref(), Some("session-123"));

    handle.remove(4242);
    assert!(handle.find_claude_launch(4242).await.is_none());
}

#[tokio::test]
async fn test_snapshot_returns_consistent_state() {
    let handle = RegistryHandle::spawn();
    let cwd = PathBuf::from("/tmp/project");

    handle.register(AgentTool::Codex, cwd.clone(), 101);
    handle.register(AgentTool::Codex, cwd.clone(), 202);
    handle.register_codex_thread("thread-abc".to_string(), 101);

    let snap = handle.snapshot().await;
    assert_eq!(snap.find_pids(AgentTool::Codex, &cwd), vec![101, 202]);
    assert_eq!(snap.find_codex_pid_for_thread("thread-abc"), Some(101));
}

#[tokio::test]
async fn test_concurrent_sends_do_not_deadlock() {
    let handle = RegistryHandle::spawn();
    let cwd = PathBuf::from("/tmp/project");

    let mut tasks = Vec::new();
    for i in 0..10_u32 {
        let h = handle.clone();
        let c = cwd.clone();
        tasks.push(tokio::spawn(async move {
            h.register(AgentTool::Claude, c, 1000 + i);
        }));
    }

    for task in tasks {
        task.await.unwrap();
    }

    // The last registration wins for non-Codex tools (single-PID override).
    let pid = handle.find_pid(AgentTool::Claude, &cwd).await;
    assert!(pid.is_some(), "should have exactly one PID registered");
}

#[tokio::test]
async fn test_sequential_codex_sends_arrive_in_order() {
    let handle = RegistryHandle::spawn();
    let cwd = PathBuf::from("/tmp/project");

    // Send registrations sequentially via the same handle to guarantee order
    for i in 0..5_u32 {
        handle.register(AgentTool::Codex, cwd.clone(), 500 + i);
    }

    let pids = handle.find_pids(AgentTool::Codex, &cwd).await;
    assert_eq!(pids, vec![500, 501, 502, 503, 504]);
}
