use super::*;
use crate::app::agent::{Agent, AgentDetail, AgentIdent, AgentLifecycle};
use crate::app::state::{OverviewState, SystemState};
use crate::event::{AgentEvent, AgentRef};
use chrono::Local;
use nexus_domain::agents::activity::Activity;
use nexus_domain::agents::ids::{AgentId, AgentTool};
use nexus_domain::agents::status::{NormalizedState, NormalizedStatus, PrimaryStatus};
use std::collections::VecDeque;
use std::path::PathBuf;

fn make_agent(tool: AgentTool, pid: u32, status: PrimaryStatus) -> Agent {
    let now = Local::now();
    Agent {
        ident: AgentIdent {
            id: AgentId::new(tool, pid),
            pid: Some(pid),
            session_id: None,
            alias: None,
            cwd: Some(PathBuf::from(format!("/tmp/{pid}"))),
            branch: None,
            is_worktree: false,
            started_at: now,
        },
        lifecycle: AgentLifecycle {
            status,
            normalized: NormalizedState::new(NormalizedStatus::Working, None),
            activity: None,
            last_activity: now,
        },
        detail: AgentDetail {
            subagents: Vec::new(),
            recent_events: VecDeque::new(),
            todos: Vec::new(),
        },
    }
}

fn make_codex_session_agent(pid: u32, session_id: &str, status: PrimaryStatus) -> Agent {
    let now = Local::now();
    Agent {
        ident: AgentIdent {
            id: AgentId::session(AgentTool::Codex, session_id),
            pid: Some(pid),
            session_id: Some(session_id.to_string()),
            alias: None,
            cwd: Some(PathBuf::from(format!("/tmp/{pid}"))),
            branch: None,
            is_worktree: false,
            started_at: now,
        },
        lifecycle: AgentLifecycle {
            status,
            normalized: NormalizedState::new(NormalizedStatus::Unknown, None),
            activity: None,
            last_activity: now,
        },
        detail: AgentDetail {
            subagents: Vec::new(),
            recent_events: VecDeque::new(),
            todos: Vec::new(),
        },
    }
}

#[test]
fn reduce_discovered_creates_new_agent() {
    let mut agents = Vec::new();
    let mut overview = OverviewState::default();
    let mut system = SystemState::default();
    let now = Local::now();

    let event = AgentEvent::Discovered {
        agent: AgentRef::new(AgentId::new(AgentTool::Claude, 42), Some(42)),
        cwd: PathBuf::from("/tmp/project"),
        started_at: None,
    };

    reduce(&mut agents, &mut overview, &mut system, &event, now);

    assert_eq!(agents.len(), 1);
    assert_eq!(agents[0].pid(), Some(42));
    assert_eq!(agents[0].status(), PrimaryStatus::Running);
    assert_eq!(agents[0].cwd(), Some(PathBuf::from("/tmp/project").as_path()));
    assert_eq!(overview.selected_index(), Some(0));
}

#[test]
fn reduce_discovered_updates_existing_agent() {
    let mut agents = vec![make_agent(AgentTool::Claude, 10, PrimaryStatus::Running)];
    let mut overview = OverviewState { selected_index: Some(0), ..Default::default() };
    let mut system = SystemState::default();
    let now = Local::now();

    let event = AgentEvent::Discovered {
        agent: AgentRef::new(agents[0].id().clone(), Some(10)),
        cwd: PathBuf::from("/new/path"),
        started_at: None,
    };

    reduce(&mut agents, &mut overview, &mut system, &event, now);

    assert_eq!(agents.len(), 1);
    assert_eq!(agents[0].cwd(), Some(PathBuf::from("/new/path").as_path()));
}

#[test]
fn reduce_status_changed_updates_agent_status() {
    let mut agents = vec![make_agent(AgentTool::Claude, 10, PrimaryStatus::Running)];
    let mut overview = OverviewState { selected_index: Some(0), ..Default::default() };
    let mut system = SystemState::default();
    let now = Local::now();

    let event = AgentEvent::StatusChanged {
        agent: AgentRef::new(agents[0].id().clone(), Some(10)),
        old: PrimaryStatus::Running,
        new: PrimaryStatus::Done,
    };

    reduce(&mut agents, &mut overview, &mut system, &event, now);

    assert_eq!(agents[0].status(), PrimaryStatus::Done);
    assert!(!agents[0].recent_events().is_empty());
    assert_eq!(
        agents[0].recent_events().back().unwrap().kind,
        crate::app::agent::ActivityEventKind::StatusChanged
    );
}

#[test]
fn reduce_process_exited_marks_session_agent_exited() {
    let mut agents = vec![make_codex_session_agent(
        50,
        "sess-1",
        PrimaryStatus::Running,
    )];
    // Also add the process agent that will trigger the exit
    let process_id = AgentId::new(AgentTool::Codex, 50);
    let mut overview = OverviewState { selected_index: Some(0), ..Default::default() };
    let mut system = SystemState::default();
    let now = Local::now();

    let event = AgentEvent::ProcessExited {
        agent: AgentRef::new(process_id, Some(50)),
    };

    reduce(&mut agents, &mut overview, &mut system, &event, now);

    // Session agent should be marked exited (not removed)
    assert_eq!(agents.len(), 1);
    assert_eq!(agents[0].status(), PrimaryStatus::Exited);
}

#[test]
fn reduce_process_exited_removes_claude_process_agents() {
    let mut agents = vec![make_agent(AgentTool::Claude, 10, PrimaryStatus::Running)];
    let agent_id = agents[0].id().clone();
    let mut overview = OverviewState { selected_index: Some(0), ..Default::default() };
    let mut system = SystemState::default();
    let now = Local::now();

    let event = AgentEvent::ProcessExited {
        agent: AgentRef::new(agent_id, Some(10)),
    };

    reduce(&mut agents, &mut overview, &mut system, &event, now);

    assert!(agents.is_empty(), "Claude process agent should be removed entirely");
}

#[test]
fn reduce_tool_call_sets_activity_and_pushes_event() {
    let mut agents = vec![make_agent(AgentTool::Claude, 10, PrimaryStatus::Running)];
    let agent_id = agents[0].id().clone();
    let mut overview = OverviewState { selected_index: Some(0), ..Default::default() };
    let mut system = SystemState::default();
    let now = Local::now();

    let event = AgentEvent::ToolCallStarted {
        agent: AgentRef::new(agent_id, Some(10)),
        tool_name: "read_file".to_string(),
        args_summary: Some("foo.rs".to_string()),
    };

    reduce(&mut agents, &mut overview, &mut system, &event, now);

    assert_eq!(agents[0].activity(), Some(Activity::ToolCall));
    assert!(!agents[0].recent_events().is_empty());
    let last_event = agents[0].recent_events().back().unwrap();
    assert_eq!(last_event.kind, crate::app::agent::ActivityEventKind::ToolCall);
    assert_eq!(last_event.description, "read_file (foo.rs)");
}

#[test]
fn reduce_detector_health_updates_root_not_agents() {
    let mut agents = vec![make_agent(AgentTool::Claude, 10, PrimaryStatus::Running)];
    let mut overview = OverviewState { selected_index: Some(0), ..Default::default() };
    let mut system = SystemState::default();
    let now = Local::now();

    let event = AgentEvent::DetectorHealthChanged {
        detector: nexus_domain::system::DetectorKind::Codex,
        healthy: false,
        message: "sqlite locked".to_string(),
    };

    let agents_before = agents.clone();
    reduce(&mut agents, &mut overview, &mut system, &event, now);

    // Agents unchanged
    assert_eq!(agents.len(), agents_before.len());
    assert_eq!(agents[0].status(), agents_before[0].status());

    // System updated
    let health = system
        .detector_health()
        .get(&nexus_domain::system::DetectorKind::Codex)
        .expect("detector health should be recorded");
    assert!(!health.healthy);
    assert_eq!(health.message, "sqlite locked");
}

#[test]
fn reduce_is_deterministic_same_input_same_output() {
    let now = Local::now();

    let run = || {
        let mut agents = Vec::new();
        let mut overview = OverviewState::default();
        let mut system = SystemState::default();

        let events = vec![
            AgentEvent::Discovered {
                agent: AgentRef::new(AgentId::new(AgentTool::Claude, 10), Some(10)),
                cwd: PathBuf::from("/tmp/project"),
                started_at: None,
            },
            AgentEvent::StatusChanged {
                agent: AgentRef::new(AgentId::new(AgentTool::Claude, 10), Some(10)),
                old: PrimaryStatus::Running,
                new: PrimaryStatus::Done,
            },
            AgentEvent::ToolCallStarted {
                agent: AgentRef::new(AgentId::new(AgentTool::Claude, 10), Some(10)),
                tool_name: "edit".to_string(),
                args_summary: None,
            },
            AgentEvent::ToolCallCompleted {
                agent: AgentRef::new(AgentId::new(AgentTool::Claude, 10), Some(10)),
            },
        ];

        for event in &events {
            reduce(&mut agents, &mut overview, &mut system, event, now);
        }

        (agents, overview)
    };

    let (agents_a, overview_a) = run();
    let (agents_b, overview_b) = run();

    assert_eq!(agents_a.len(), agents_b.len());
    assert_eq!(overview_a.selected_index(), overview_b.selected_index());

    for (a, b) in agents_a.iter().zip(agents_b.iter()) {
        assert_eq!(a.id(), b.id());
        assert_eq!(a.pid(), b.pid());
        assert_eq!(a.status(), b.status());
        assert_eq!(a.activity(), b.activity());
        assert_eq!(a.cwd(), b.cwd());
        assert_eq!(a.recent_events().len(), b.recent_events().len());
    }
}
