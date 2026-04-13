use std::collections::VecDeque;
use std::path::PathBuf;

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
    use chrono::Local;

    #[test]
    fn push_event_caps_recent_event_history() {
        let now = Local::now();
        let mut events = VecDeque::new();

        for idx in 0..(MAX_RECENT_EVENTS + 3) {
            push_event(
                &mut events,
                ActivityEventKind::StatusChanged,
                format!("event-{idx}"),
                now,
            );
        }

        assert_eq!(events.len(), MAX_RECENT_EVENTS);
        assert_eq!(events.front().map(|event| event.description.as_str()), Some("event-3"));
        assert_eq!(
            events.back().map(|event| event.description.as_str()),
            Some("event-22")
        );
    }
}
