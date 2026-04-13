pub mod accounts;
pub mod activity;
pub mod agent_list;
pub mod detail;
pub mod footer;
pub mod header;
pub mod help;
pub mod overview;
pub mod shared;
pub mod theme;

use ratatui::Frame;

use crate::app::{ActiveTab, AppState};
use crate::domain::agents::model::Agent;
use crate::domain::agents::status::NormalizedStatus;
use crate::services::runtime::RuntimeSession;

pub struct HeaderStats {
    pub total: usize,
    pub running: usize,
    pub waiting: usize,
    pub error: usize,
    pub done: usize,
    pub subagent_count: usize,
}

pub struct RenderData<'a> {
    pub active_tab: ActiveTab,
    pub visible_agents: Vec<&'a Agent>,
    pub selected_agent: Option<&'a Agent>,
    pub header: HeaderStats,
}

impl<'a> RenderData<'a> {
    fn from_state(state: &'a AppState) -> Self {
        let visible_agents = state.visible_agents();
        let selected_agent = state
            .root()
            .overview()
            .selected_index()
            .and_then(|i| visible_agents.get(i).copied());
        let count_visible_with_normalized_status = |status| {
            visible_agents
                .iter()
                .filter(|agent| agent.normalized.status == status)
                .count()
        };
        let error_count = visible_agents
            .iter()
            .filter(|agent| {
                matches!(
                    agent.normalized.status,
                    NormalizedStatus::Blocked | NormalizedStatus::Unknown
                )
            })
            .count();

        let header = HeaderStats {
            total: visible_agents.len(),
            running: count_visible_with_normalized_status(NormalizedStatus::Working),
            waiting: count_visible_with_normalized_status(NormalizedStatus::NeedsInput),
            error: error_count,
            done: count_visible_with_normalized_status(NormalizedStatus::Done),
            subagent_count: visible_agents
                .iter()
                .map(|agent| agent.subagents.len())
                .sum(),
        };

        Self {
            active_tab: state.active_tab(),
            visible_agents,
            selected_agent,
            header,
        }
    }
}

pub fn render(frame: &mut Frame, state: &RuntimeSession) {
    let area = frame.area();
    if area.width < 40 || area.height < 8 {
        shared::layout::render_small_terminal_message(frame);
        return;
    }

    match state.active_tab() {
        ActiveTab::Overview => {
            let render_data = RenderData::from_state(state);
            overview::render(frame, state, &render_data)
        }
        ActiveTab::Accounts => accounts::render(frame, state),
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::app::AppState;
    use crate::domain::agents::status::{NormalizedReason, NormalizedState, NormalizedStatus};
    use crate::domain::agents::{AgentId, AgentTool};
    use crate::event::AgentEvent;
    use crate::services::runtime::RuntimeSession;

    use super::RenderData;

    #[test]
    fn workspace_render_modules_export_entrypoints() {
        let _overview_render: fn(&mut ratatui::Frame, &RuntimeSession, &super::RenderData<'_>) =
            crate::ui::overview::render;
        let _accounts_render: fn(&mut ratatui::Frame, &RuntimeSession) =
            crate::ui::accounts::render;
        let _accounts_list_render: fn(&mut ratatui::Frame, &RuntimeSession, ratatui::layout::Rect) =
            crate::ui::accounts::list::render;
        let _accounts_detail_render: fn(
            &mut ratatui::Frame,
            &RuntimeSession,
            ratatui::layout::Rect,
        ) = crate::ui::accounts::detail::render;
        let _accounts_edit_render: fn(
            &mut ratatui::Frame,
            &crate::app::AppState,
            ratatui::layout::Rect,
        ) = crate::ui::accounts::edit::render;
        let _accounts_confirm_render: fn(
            &mut ratatui::Frame,
            &crate::app::AppState,
            &crate::app::ConfirmKind,
        ) = crate::ui::accounts::confirm::render;
        let _help_render: fn(&mut ratatui::Frame, u16) = crate::ui::shared::help::render;
    }

    #[test]
    fn header_counts_only_visible_agents() {
        let mut state = AppState::new();

        state.handle_event(AgentEvent::Discovered {
            id: AgentId::new(AgentTool::Claude, 10),
            pid: Some(10),
            cwd: PathBuf::from("/tmp/visible"),
            started_at: None,
        });
        state.handle_event(AgentEvent::SessionInfo {
            id: AgentId::session(AgentTool::Codex, "hidden-codex"),
            pid: None,
            session_id: "hidden-codex".to_string(),
            branch: Some("main".to_string()),
            is_worktree: false,
            started_at: None,
        });

        let render_data = RenderData::from_state(&state);

        assert_eq!(render_data.visible_agents.len(), 1);
        assert_eq!(render_data.header.total, 1);
        assert_eq!(render_data.header.subagent_count, 0);
    }

    #[test]
    fn header_status_counts_follow_active_filter() {
        let mut state = AppState::new();

        state.handle_event(AgentEvent::Discovered {
            id: AgentId::new(AgentTool::Claude, 10),
            pid: Some(10),
            cwd: PathBuf::from("/tmp/claude"),
            started_at: None,
        });
        state.handle_event(AgentEvent::Discovered {
            id: AgentId::new(AgentTool::OpenCode, 11),
            pid: Some(11),
            cwd: PathBuf::from("/tmp/opencode"),
            started_at: None,
        });
        state.handle_event(AgentEvent::NormalizedStateChanged {
            id: AgentId::new(AgentTool::OpenCode, 11),
            pid: Some(11),
            state: NormalizedState::new(
                NormalizedStatus::NeedsInput,
                Some(NormalizedReason::Review),
            ),
        });
        state.start_filter();
        for ch in "claude".chars() {
            state.filter_push_char(ch);
        }
        state.confirm_filter();

        let render_data = RenderData::from_state(&state);

        assert_eq!(render_data.header.total, 1);
        assert_eq!(render_data.header.running, 1);
        assert_eq!(render_data.header.waiting, 0);
    }

    #[test]
    fn header_counts_normalized_states_and_maps_unknown_to_error_bucket() {
        let mut state = AppState::new();

        state.handle_event(AgentEvent::Discovered {
            id: AgentId::new(AgentTool::Claude, 10),
            pid: Some(10),
            cwd: PathBuf::from("/tmp/claude"),
            started_at: None,
        });
        state.handle_event(AgentEvent::Discovered {
            id: AgentId::new(AgentTool::OpenCode, 11),
            pid: Some(11),
            cwd: PathBuf::from("/tmp/opencode"),
            started_at: None,
        });
        state.handle_event(AgentEvent::Discovered {
            id: AgentId::new(AgentTool::Codex, 12),
            pid: Some(12),
            cwd: PathBuf::from("/tmp/codex"),
            started_at: None,
        });
        state.handle_event(AgentEvent::Discovered {
            id: AgentId::new(AgentTool::Codex, 13),
            pid: Some(13),
            cwd: PathBuf::from("/tmp/codex-unknown"),
            started_at: None,
        });

        state.handle_event(AgentEvent::NormalizedStateChanged {
            id: AgentId::new(AgentTool::Claude, 10),
            pid: Some(10),
            state: NormalizedState::new(
                NormalizedStatus::NeedsInput,
                Some(NormalizedReason::Review),
            ),
        });
        state.handle_event(AgentEvent::NormalizedStateChanged {
            id: AgentId::new(AgentTool::OpenCode, 11),
            pid: Some(11),
            state: NormalizedState::new(
                NormalizedStatus::Done,
                Some(NormalizedReason::ReadyForNextTask),
            ),
        });

        let render_data = RenderData::from_state(&state);

        assert_eq!(render_data.header.total, 4);
        assert_eq!(render_data.header.running, 0);
        assert_eq!(render_data.header.waiting, 1);
        assert_eq!(render_data.header.error, 2);
        assert_eq!(render_data.header.done, 1);
    }

    #[test]
    fn header_counts_blocked_in_error_bucket() {
        let mut state = AppState::new();

        state.handle_event(AgentEvent::Discovered {
            id: AgentId::new(AgentTool::Claude, 10),
            pid: Some(10),
            cwd: PathBuf::from("/tmp/claude"),
            started_at: None,
        });
        state.handle_event(AgentEvent::NormalizedStateChanged {
            id: AgentId::new(AgentTool::Claude, 10),
            pid: Some(10),
            state: NormalizedState::new(
                NormalizedStatus::Blocked,
                Some(NormalizedReason::Approval),
            ),
        });

        let render_data = RenderData::from_state(&state);

        assert_eq!(render_data.header.running, 0);
        assert_eq!(render_data.header.waiting, 0);
        assert_eq!(render_data.header.error, 1);
        assert_eq!(render_data.header.done, 0);
    }
}
