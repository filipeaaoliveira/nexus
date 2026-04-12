use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};
use ratatui::Frame;

use crate::ui::theme;

pub fn render_small_terminal_message(frame: &mut Frame) {
    let area = frame.area();
    frame.render_widget(
        Block::default().style(Style::default().bg(theme::BG_MAIN)),
        area,
    );
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            " Terminal too small — resize to continue",
            Style::default().fg(theme::FG_COMMENT),
        ))),
        area,
    );
}
