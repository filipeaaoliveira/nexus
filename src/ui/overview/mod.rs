pub mod activity;
pub mod detail;
pub mod footer;
pub mod list;

use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::Style;
use ratatui::widgets::Block;
use ratatui::Frame;

use crate::services::runtime::RuntimeSession;

use crate::ui::{header, shared, theme, RenderData};

pub fn render(frame: &mut Frame, state: &RuntimeSession, render_data: &RenderData<'_>) {
    frame.render_widget(
        Block::default().style(Style::default().bg(theme::BG_MAIN)),
        frame.area(),
    );

    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(frame.area());

    header::render(frame, state, render_data, outer[0]);

    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
        .split(outer[1]);

    list::render(frame, state, render_data, main_chunks[0]);

    let right_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(main_chunks[1]);

    detail::render(frame, render_data, right_chunks[0]);
    activity::render(frame, render_data, right_chunks[1]);
    footer::render(frame, state.app(), outer[2]);

    if state.show_help() {
        shared::help::render(frame, state.help_scroll());
    }
}
