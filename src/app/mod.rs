use std::collections::VecDeque;
use std::path::{Path, PathBuf};

pub use crate::domain::accounts::Provider;
#[cfg(test)]
pub use crate::domain::accounts::{AccountEntry, ProviderFilter};
pub use crate::domain::agents::activity::{
    Activity, ActivityEvent, ActivityEventKind, Subagent, TodoItem,
};
pub use crate::domain::agents::ids::{AgentId, AgentTool};
pub use crate::domain::agents::model::Agent;
pub use crate::domain::agents::status::{NormalizedState, NormalizedStatus, PrimaryStatus};

pub mod action;
mod core;
pub mod dispatch;
pub mod effect;
pub mod reducer;
pub mod state;

mod accounts;
mod events;
mod overview;
mod system;

pub use action::{
    AccountsAction, Action, DetectorAction, OverviewAction, SystemAction, WorkspaceAction,
};
pub use core::AppState;
#[cfg(test)]
pub use effect::AccountProfile;
pub use effect::{Effect, EffectResult, RefreshOrigin};
pub use state::Workspace as ActiveTab;
pub use state::{
    AccountsConfirmAction, AccountsEditField, AccountsMode, Overlay, OverviewMode, ProfileDraft,
    RootState, Workspace,
};
#[cfg(test)]
pub use state::{AccountsState, OverviewStatusCounts, Workflow};

const MAX_RECENT_EVENTS: usize = 20;

/// Which destructive action is awaiting confirmation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmKind {
    DeleteProfile,
    WriteToHome,
}

#[derive(Debug, Clone)]
struct SessionInfoUpdate {
    pid: Option<u32>,
    session_id: String,
    branch: Option<String>,
    is_worktree: bool,
    started_at: Option<chrono::DateTime<chrono::Local>>,
}

pub(super) fn push_event(
    events: &mut VecDeque<ActivityEvent>,
    kind: ActivityEventKind,
    description: String,
    now: chrono::DateTime<chrono::Local>,
) {
    events.push_back(ActivityEvent {
        timestamp: now,
        kind,
        description,
    });
    while events.len() > MAX_RECENT_EVENTS {
        events.pop_front();
    }
}

#[cfg(test)]
mod invariant_tests {
    use super::*;
    use crate::event::AgentEvent;

    fn discovered(tool: AgentTool, pid: u32, cwd: &str) -> AgentEvent {
        AgentEvent::Discovered {
            id: AgentId::new(tool, pid),
            pid: Some(pid),
            cwd: PathBuf::from(cwd),
            started_at: None,
        }
    }

    #[test]
    fn overview_projection_tracks_visible_agents_without_join_drift() {
        let mut state = AppState::new();
        state.handle_event(discovered(AgentTool::Claude, 100, "/tmp/one"));
        state.handle_event(discovered(AgentTool::OpenCode, 200, "/tmp/two"));

        let overview_ids: Vec<_> = state
            .root
            .overview
            .visible_agents()
            .into_iter()
            .map(|agent| agent.id.clone())
            .collect();
        let visible_ids: Vec<_> = state
            .visible_agents()
            .into_iter()
            .map(|agent| agent.id.clone())
            .collect();

        assert_eq!(overview_ids, visible_ids);
        assert_eq!(visible_ids.len(), 2);
        assert_eq!(visible_ids[0], AgentId::new(AgentTool::Claude, 100));
        assert_eq!(visible_ids[1], AgentId::new(AgentTool::OpenCode, 200));
        assert_eq!(
            state.root.overview.selected_agent_id(),
            Some(AgentId::new(AgentTool::Claude, 100))
        );
    }

    #[test]
    fn codex_session_without_pid_stays_hidden_until_process_binding_exists() {
        let mut state = AppState::new();

        state.handle_event(AgentEvent::SessionInfo {
            id: AgentId::session(AgentTool::Codex, "thread-1"),
            pid: None,
            session_id: "thread-1".into(),
            branch: Some("main".into()),
            is_worktree: false,
            started_at: None,
        });

        assert!(state.visible_agents().is_empty());
        assert_eq!(state.root.overview.selected_agent_id(), None);

        state.handle_event(AgentEvent::SessionInfo {
            id: AgentId::session(AgentTool::Codex, "thread-1"),
            pid: Some(44),
            session_id: "thread-1".into(),
            branch: Some("main".into()),
            is_worktree: false,
            started_at: None,
        });

        assert_eq!(state.visible_agents().len(), 1);
        assert_eq!(
            state.root.overview.selected_agent_id(),
            Some(AgentId::session(AgentTool::Codex, "thread-1"))
        );
    }

    #[test]
    fn discovered_then_session_info_produces_one_logical_agent() {
        let mut state = AppState::new();

        state.handle_event(AgentEvent::Discovered {
            id: AgentId::new(AgentTool::Codex, 44),
            pid: Some(44),
            cwd: PathBuf::from("/tmp/repo"),
            started_at: None,
        });
        state.handle_event(AgentEvent::SessionInfo {
            id: AgentId::session(AgentTool::Codex, "thread-1"),
            pid: Some(44),
            session_id: "thread-1".into(),
            branch: Some("main".into()),
            is_worktree: false,
            started_at: None,
        });

        assert_eq!(state.visible_agents().len(), 1);
        let agent = state.visible_agents()[0];
        assert!(agent.id.is_session());
        assert_eq!(agent.pid, Some(44));
        assert_eq!(agent.cwd, PathBuf::from("/tmp/repo"));
    }

    #[test]
    fn session_info_then_discovered_merges_into_existing_session_row() {
        let mut state = AppState::new();

        state.handle_event(AgentEvent::SessionInfo {
            id: AgentId::session(AgentTool::Codex, "thread-1"),
            pid: Some(44),
            session_id: "thread-1".into(),
            branch: Some("main".into()),
            is_worktree: false,
            started_at: None,
        });
        state.handle_event(AgentEvent::Discovered {
            id: AgentId::new(AgentTool::Codex, 44),
            pid: Some(44),
            cwd: PathBuf::from("/tmp/repo"),
            started_at: None,
        });

        assert_eq!(state.visible_agents().len(), 1);
        let agent = state.visible_agents()[0];
        assert_eq!(agent.id, AgentId::session(AgentTool::Codex, "thread-1"));
        assert_eq!(agent.cwd, PathBuf::from("/tmp/repo"));
        assert_eq!(agent.session_id.as_deref(), Some("thread-1"));
    }

    #[test]
    fn process_exit_removes_codex_process_only_rows_from_visibility() {
        let mut state = AppState::new();

        state.handle_event(AgentEvent::Discovered {
            id: AgentId::new(AgentTool::Codex, 88),
            pid: Some(88),
            cwd: PathBuf::from("/tmp/repo"),
            started_at: None,
        });
        assert_eq!(state.visible_agents().len(), 1);

        state.handle_event(AgentEvent::ProcessExited {
            id: AgentId::new(AgentTool::Codex, 88),
            pid: Some(88),
        });

        assert!(state.visible_agents().is_empty());
    }
}
