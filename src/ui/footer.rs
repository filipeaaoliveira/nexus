use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::AppState;

use super::theme;

pub fn render(frame: &mut Frame, state: &AppState, area: Rect) {
    if state.filter_mode {
        let line = Line::from(vec![
            Span::styled(" / ", theme::bar_style().fg(theme::STATUS_WAITING)),
            Span::styled(
                &state.filter_input,
                theme::bar_style().fg(theme::FG_DEFAULT),
            ),
            Span::styled("_", theme::bar_style().fg(theme::STATUS_WAITING)),
        ]);
        let footer = Paragraph::new(line).style(theme::bar_style());
        frame.render_widget(footer, area);
        return;
    }

    let sort_label = state.sort_mode.as_str();
    let line = Line::from(vec![
        Span::styled(" j/k", theme::bar_style().fg(theme::FG_DEFAULT)),
        Span::styled(" navigate", theme::bar_style().fg(theme::FG_DIMMED)),
        Span::styled("  /", theme::bar_style().fg(theme::FG_DEFAULT)),
        Span::styled(" filter", theme::bar_style().fg(theme::FG_DIMMED)),
        Span::styled("  g", theme::bar_style().fg(theme::FG_DEFAULT)),
        Span::styled(" group", theme::bar_style().fg(theme::FG_DIMMED)),
        Span::styled("  s", theme::bar_style().fg(theme::FG_DEFAULT)),
        Span::styled(
            format!(" sort:{sort_label}"),
            theme::bar_style().fg(theme::FG_DIMMED),
        ),
        Span::styled("  d", theme::bar_style().fg(theme::FG_DEFAULT)),
        Span::styled("/", theme::bar_style().fg(theme::FG_DEFAULT)),
        Span::styled("D", theme::bar_style().fg(theme::FG_DEFAULT)),
        Span::styled(" remove", theme::bar_style().fg(theme::FG_DIMMED)),
        Span::styled("  ?", theme::bar_style().fg(theme::FG_DEFAULT)),
        Span::styled(" help", theme::bar_style().fg(theme::FG_DIMMED)),
        Span::styled("  q", theme::bar_style().fg(theme::FG_DEFAULT)),
        Span::styled(" quit", theme::bar_style().fg(theme::FG_DIMMED)),
    ]);

    let paragraph =
        Paragraph::new(line).style(ratatui::style::Style::default().bg(theme::BG_HEADER));
    frame.render_widget(paragraph, area);
}
