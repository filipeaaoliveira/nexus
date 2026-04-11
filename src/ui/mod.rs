pub mod accounts_tab;
pub mod activity;
pub mod agent_list;
pub mod detail;
pub mod footer;
pub mod header;
pub mod help;
pub mod theme;

use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::{ActiveTab, AppState};
use crate::types::{Agent, NormalizedStatus};

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
        let selected_agent = state.selected.and_then(|i| visible_agents.get(i).copied());
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

pub fn render(frame: &mut Frame, state: &AppState) {
    let area = frame.area();
    if area.width < 40 || area.height < 8 {
        frame.render_widget(
            ratatui::widgets::Block::default().style(Style::default().bg(theme::BG_MAIN)),
            area,
        );
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                " Terminal too small — resize to continue",
                Style::default().fg(theme::FG_COMMENT),
            ))),
            area,
        );
        return;
    }

    match state.active_tab() {
        ActiveTab::Overview => {
            let render_data = RenderData::from_state(state);
            render_overview(frame, state, &render_data)
        }
        ActiveTab::Accounts => accounts_tab::render(frame, state),
    }
}

fn render_overview(frame: &mut Frame, state: &AppState, render_data: &RenderData<'_>) {
    // Fill background
    frame.render_widget(
        ratatui::widgets::Block::default().style(Style::default().bg(theme::BG_MAIN)),
        frame.area(),
    );

    // Top-level vertical layout: header, main, footer
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // header
            Constraint::Min(0),    // main
            Constraint::Length(1), // footer
        ])
        .split(frame.area());

    // Header
    header::render(frame, state, render_data, outer[0]);

    // Main: split horizontally into agent list (35%) and right pane (65%)
    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
        .split(outer[1]);

    // Agent list (left)
    agent_list::render(frame, state, render_data, main_chunks[0]);

    // Right pane: split vertically into detail (40%) and activity (60%)
    let right_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(main_chunks[1]);

    // Detail (right top)
    detail::render(frame, render_data, right_chunks[0]);

    // Activity (right bottom)
    activity::render(frame, render_data, right_chunks[1]);

    // Footer
    footer::render(frame, state, outer[2]);

    // Help overlay
    if state.show_help {
        help::render(frame, state.help_scroll);
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::app::AppState;
    use crate::config::Config;
    use crate::event::AgentEvent;
    use crate::types::AgentId;
    use crate::types::AgentTool;
    use crate::types::{NormalizedReason, NormalizedState, NormalizedStatus};

    use super::RenderData;

    #[test]
    fn header_counts_only_visible_agents() {
        let mut state = AppState::new(Config::default());

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
        let mut state = AppState::new(Config::default());

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
        state.filter = Some("claude".to_string());

        let render_data = RenderData::from_state(&state);

        assert_eq!(render_data.header.total, 1);
        assert_eq!(render_data.header.running, 1);
        assert_eq!(render_data.header.waiting, 0);
    }

    #[test]
    fn header_counts_normalized_states_and_maps_unknown_to_error_bucket() {
        let mut state = AppState::new(Config::default());

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
        let mut state = AppState::new(Config::default());

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
