use std::collections::VecDeque;

pub use nexus_domain::accounts::Provider;
#[cfg(test)]
pub use nexus_domain::accounts::{AccountEntry, ProviderFilter};
pub use nexus_domain::agents::ids::AgentId;
#[cfg(test)]
pub use nexus_domain::agents::status::{NormalizedState, NormalizedStatus, PrimaryStatus};

pub use agent::{ActivityEvent, ActivityEventKind, Agent};
#[cfg(test)]
pub use agent::{AgentDetail, AgentIdent, AgentLifecycle};

pub mod action;
pub mod agent;
mod core;
pub mod dispatch;
pub mod effect;
pub mod reducer;
pub mod state;

mod accounts;
mod overview;
pub mod projection;
mod system;

pub use action::{AccountsAction, Action, OverviewAction, SystemAction, WorkspaceAction};
pub use core::AppState;
pub use effect::{Effect, EffectResult, RefreshOrigin};
pub use state::{
    AccountsConfirmAction, AccountsEditField, AccountsMode, EditableProfile, Overlay, OverviewMode,
    RootState, Workspace,
};

const MAX_RECENT_EVENTS: usize = 20;

/// Which destructive action is awaiting confirmation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmKind {
    DeleteProfile,
    WriteToHome,
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
        let overflow = 3;

        for idx in 0..(MAX_RECENT_EVENTS + overflow) {
            push_event(
                &mut events,
                ActivityEventKind::StatusChanged,
                format!("event-{idx}"),
                now,
            );
        }

        assert_eq!(events.len(), MAX_RECENT_EVENTS);
        let first_kept = format!("event-{overflow}");
        let last_kept = format!("event-{}", MAX_RECENT_EVENTS + overflow - 1);
        assert_eq!(
            events.front().map(|event| event.description.as_str()),
            Some(first_kept.as_str())
        );
        assert_eq!(
            events.back().map(|event| event.description.as_str()),
            Some(last_kept.as_str())
        );
    }
}
