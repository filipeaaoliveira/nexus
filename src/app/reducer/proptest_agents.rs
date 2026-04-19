use super::*;
use crate::app::agent::{Agent, AgentDetail, AgentIdent, AgentLifecycle};
use crate::app::state::{OverviewState, SystemState};
use crate::event::AgentEvent;
use chrono::{DateTime, Local, TimeZone};
use nexus_domain::agents::activity::Activity;
use nexus_domain::agents::ids::{AgentId, AgentTool};
use nexus_domain::agents::status::{NormalizedState, NormalizedStatus, PrimaryStatus};
use proptest::prelude::*;
use std::collections::VecDeque;
use std::path::PathBuf;

fn fixed_now() -> DateTime<Local> {
    Local.with_ymd_and_hms(2026, 4, 19, 12, 0, 0).unwrap()
}

fn arb_tool() -> impl Strategy<Value = AgentTool> {
    prop_oneof![
        Just(AgentTool::Claude),
        Just(AgentTool::Codex),
        Just(AgentTool::OpenCode),
    ]
}

fn arb_status() -> impl Strategy<Value = PrimaryStatus> {
    prop_oneof![
        Just(PrimaryStatus::Running),
        Just(PrimaryStatus::Waiting),
        Just(PrimaryStatus::Done),
        Just(PrimaryStatus::Exited),
    ]
}

fn arb_normalized_status() -> impl Strategy<Value = NormalizedStatus> {
    prop_oneof![
        Just(NormalizedStatus::Working),
        Just(NormalizedStatus::NeedsInput),
        Just(NormalizedStatus::Blocked),
        Just(NormalizedStatus::Done),
        Just(NormalizedStatus::Unknown),
    ]
}

fn arb_activity() -> impl Strategy<Value = Option<Activity>> {
    prop_oneof![
        Just(None),
        Just(Some(Activity::Generating)),
        Just(Some(Activity::ToolCall)),
        Just(Some(Activity::Idle)),
        Just(Some(Activity::Thinking)),
        Just(Some(Activity::Compacting)),
    ]
}

fn arb_agent() -> impl Strategy<Value = Agent> {
    (arb_tool(), 1u32..100, arb_status(), arb_normalized_status(), arb_activity()).prop_map(
        |(tool, pid, status, norm_status, activity)| {
            let now = fixed_now();
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
                    normalized: NormalizedState::new(norm_status, None),
                    activity,
                    last_activity: now,
                },
                detail: AgentDetail {
                    subagents: Vec::new(),
                    recent_events: VecDeque::new(),
                    todos: Vec::new(),
                },
            }
        },
    )
}

fn arb_event() -> impl Strategy<Value = AgentEvent> {
    (arb_tool(), 1u32..100).prop_flat_map(|(tool, pid)| {
        prop_oneof![
            Just(AgentEvent::Discovered {
                id: AgentId::new(tool, pid),
                pid: Some(pid),
                cwd: PathBuf::from("/tmp/test"),
                started_at: None,
            }),
            (arb_status(), arb_status()).prop_map(move |(old, new)| {
                AgentEvent::StatusChanged {
                    id: AgentId::new(tool, pid),
                    pid: Some(pid),
                    old,
                    new,
                }
            }),
            arb_activity().prop_map(move |activity| AgentEvent::ActivityChanged {
                id: AgentId::new(tool, pid),
                pid: Some(pid),
                activity,
            }),
            Just(AgentEvent::ToolCallStarted {
                id: AgentId::new(tool, pid),
                pid: Some(pid),
                tool_name: "Read".to_string(),
                args_summary: None,
            }),
            Just(AgentEvent::ToolCallCompleted {
                id: AgentId::new(tool, pid),
                pid: Some(pid),
            }),
            Just(AgentEvent::ProcessExited {
                id: AgentId::new(tool, pid),
                pid: Some(pid),
            }),
        ]
    })
}

proptest! {
    #[test]
    fn reducer_is_deterministic(
        agents in proptest::collection::vec(arb_agent(), 0..5),
        event in arb_event(),
    ) {
        let now = fixed_now();
        let mut agents_a = agents.clone();
        let mut agents_b = agents;
        let mut overview_a = OverviewState::default();
        let mut overview_b = OverviewState::default();
        let mut system_a = SystemState::default();
        let mut system_b = SystemState::default();

        reduce(&mut agents_a, &mut overview_a, &mut system_a, &event, now);
        reduce(&mut agents_b, &mut overview_b, &mut system_b, &event, now);

        // Same number of agents
        prop_assert_eq!(agents_a.len(), agents_b.len());
        // Same selection state
        prop_assert_eq!(overview_a.selected_index(), overview_b.selected_index());
        // Same agent statuses
        for (a, b) in agents_a.iter().zip(agents_b.iter()) {
            prop_assert_eq!(a.status(), b.status());
            prop_assert_eq!(a.pid(), b.pid());
            prop_assert_eq!(a.last_activity(), b.last_activity());
        }
    }

    #[test]
    fn selection_always_in_bounds_after_reduce(
        agents in proptest::collection::vec(arb_agent(), 0..10),
        event in arb_event(),
    ) {
        let now = fixed_now();
        let mut agents = agents;
        let mut overview = OverviewState::default();
        let mut system = SystemState::default();
        if !agents.is_empty() {
            overview.selected_index = Some(0);
        }

        reduce(&mut agents, &mut overview, &mut system, &event, now);

        if let Some(idx) = overview.selected_index() {
            let visible_count = crate::app::projection::visible_agents(&agents, &overview).len();
            prop_assert!(
                idx < visible_count,
                "selected_index {} out of bounds for {} visible agents",
                idx, visible_count
            );
        }
    }
}
