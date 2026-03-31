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

pub fn render(frame: &mut Frame, state: &AppState) {
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
    header::render(frame, state, outer[0]);

    // Main: split horizontally into agent list (35%) and right pane (65%)
    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
        .split(outer[1]);

    // Agent list (left)
    agent_list::render(frame, state, main_chunks[0]);

    // Right pane: split vertically into detail (40%) and activity (60%)
    let right_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(main_chunks[1]);

    // Detail (right top)
    detail::render(frame, state, right_chunks[0]);

    // Activity (right bottom)
    activity::render(frame, state, right_chunks[1]);

    // Footer
    footer::render(frame, state, outer[2]);

    // Help overlay
    if state.show_help {
        help::render(frame);
    }
}
