pub mod activity;
pub mod agent_list;
pub mod detail;
pub mod footer;
pub mod header;
pub mod help;
pub mod theme;

use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::Style;
use ratatui::Frame;

use crate::app::AppState;
use crate::types::{Agent, PrimaryStatus};

pub struct HeaderStats {
    pub total: usize,
    pub running: usize,
    pub waiting: usize,
    pub error: usize,
    pub done: usize,
    pub exited: usize,
    pub subagent_count: usize,
}

pub struct RenderData<'a> {
    pub visible_agents: Vec<&'a Agent>,
    pub selected_agent: Option<&'a Agent>,
    pub header: HeaderStats,
}

impl<'a> RenderData<'a> {
    fn from_state(state: &'a AppState) -> Self {
        let visible_agents = state.visible_agents();
        let selected_agent = state.selected.and_then(|i| visible_agents.get(i).copied());
        let count_visible_with_status = |status| {
            visible_agents
                .iter()
                .filter(|agent| agent.status == status)
                .count()
        };

        let header = HeaderStats {
            total: visible_agents.len(),
            running: count_visible_with_status(PrimaryStatus::Running),
            waiting: count_visible_with_status(PrimaryStatus::Waiting),
            error: count_visible_with_status(PrimaryStatus::Error),
            done: count_visible_with_status(PrimaryStatus::Done),
            exited: count_visible_with_status(PrimaryStatus::Exited),
            subagent_count: visible_agents
                .iter()
                .map(|agent| agent.subagents.len())
                .sum(),
        };

        Self {
            visible_agents,
            selected_agent,
            header,
        }
    }
}

pub fn render(frame: &mut Frame, state: &AppState) {
    let render_data = RenderData::from_state(state);

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
    header::render(frame, state, &render_data, outer[0]);

    // Main: split horizontally into agent list (35%) and right pane (65%)
    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
        .split(outer[1]);

    // Agent list (left)
    agent_list::render(frame, state, &render_data, main_chunks[0]);

    // Right pane: split vertically into detail (40%) and activity (60%)
    let right_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(main_chunks[1]);

    // Detail (right top)
    detail::render(frame, &render_data, right_chunks[0]);

    // Activity (right bottom)
    activity::render(frame, &render_data, right_chunks[1]);

    // Footer
    footer::render(frame, state, outer[2]);

    // Help overlay
    if state.show_help {
        help::render(frame);
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
        state.handle_event(AgentEvent::StatusChanged {
            id: AgentId::new(AgentTool::OpenCode, 11),
            pid: Some(11),
            old: crate::types::PrimaryStatus::Running,
            new: crate::types::PrimaryStatus::Waiting,
        });
        state.filter = Some("claude".to_string());

        let render_data = RenderData::from_state(&state);

        assert_eq!(render_data.header.total, 1);
        assert_eq!(render_data.header.running, 1);
        assert_eq!(render_data.header.waiting, 0);
    }
}
