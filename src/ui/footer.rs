use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::AppState;

use super::theme;

pub fn render(frame: &mut Frame, state: &AppState, area: Rect) {
    if state.rename_mode {
        let line = Line::from(vec![
            Span::styled(" rename: ", theme::bar_style().fg(theme::STATUS_WAITING)),
            Span::styled(
                &state.rename_input,
                theme::bar_style().fg(theme::FG_DEFAULT),
            ),
            Span::styled("_", theme::bar_style().fg(theme::STATUS_WAITING)),
        ]);
        let footer = Paragraph::new(line).style(theme::bar_style());
        frame.render_widget(footer, area);
        return;
    }

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

    if state.codex_tab.filter_mode {
        let line = Line::from(vec![
            Span::styled(" / ", theme::bar_style().fg(theme::STATUS_WAITING)),
            Span::styled(
                &state.codex_tab.filter_input,
                theme::bar_style().fg(theme::FG_DEFAULT),
            ),
            Span::styled("_", theme::bar_style().fg(theme::STATUS_WAITING)),
        ]);
        let footer = Paragraph::new(line).style(theme::bar_style());
        frame.render_widget(footer, area);
        return;
    }

    let line = shortcut_line(state);

    let paragraph =
        Paragraph::new(line).style(ratatui::style::Style::default().bg(theme::BG_HEADER));
    frame.render_widget(paragraph, area);
}

pub fn shortcut_text(state: &AppState) -> String {
    shortcut_line(state)
        .spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect()
}

fn shortcut_line(state: &AppState) -> Line<'static> {
    match state.active_tab() {
        crate::app::ActiveTab::Overview => overview_shortcut_line(state),
        crate::app::ActiveTab::Codex => codex_shortcut_line(),
    }
}

fn overview_shortcut_line(state: &AppState) -> Line<'static> {
    let sort_label = state.sort_mode.as_str();
    Line::from(vec![
        Span::styled(" j/k", theme::bar_style().fg(theme::FG_DEFAULT)),
        Span::styled(" navigate", theme::bar_style().fg(theme::FG_DIMMED)),
        Span::styled("  Tab", theme::bar_style().fg(theme::FG_DEFAULT)),
        Span::styled(" switch", theme::bar_style().fg(theme::FG_DIMMED)),
        Span::styled("  Shift+Tab", theme::bar_style().fg(theme::FG_DEFAULT)),
        Span::styled(" back", theme::bar_style().fg(theme::FG_DIMMED)),
        Span::styled("  /", theme::bar_style().fg(theme::FG_DEFAULT)),
        Span::styled(" filter", theme::bar_style().fg(theme::FG_DIMMED)),
        Span::styled("  s", theme::bar_style().fg(theme::FG_DEFAULT)),
        Span::styled(
            format!(" sort:{sort_label}"),
            theme::bar_style().fg(theme::FG_DIMMED),
        ),
        Span::styled("  d", theme::bar_style().fg(theme::FG_DEFAULT)),
        Span::styled("/", theme::bar_style().fg(theme::FG_DEFAULT)),
        Span::styled("D", theme::bar_style().fg(theme::FG_DEFAULT)),
        Span::styled(" remove", theme::bar_style().fg(theme::FG_DIMMED)),
        Span::styled("  r", theme::bar_style().fg(theme::FG_DEFAULT)),
        Span::styled(" rename", theme::bar_style().fg(theme::FG_DIMMED)),
        Span::styled("  ?", theme::bar_style().fg(theme::FG_DEFAULT)),
        Span::styled(" help", theme::bar_style().fg(theme::FG_DIMMED)),
        Span::styled("  q", theme::bar_style().fg(theme::FG_DEFAULT)),
        Span::styled(" quit", theme::bar_style().fg(theme::FG_DIMMED)),
    ])
}

fn codex_shortcut_line() -> Line<'static> {
    Line::from(vec![
        Span::styled(" j/k", theme::bar_style().fg(theme::FG_DEFAULT)),
        Span::styled(" profiles", theme::bar_style().fg(theme::FG_DIMMED)),
        Span::styled("  v", theme::bar_style().fg(theme::FG_DEFAULT)),
        Span::styled(" view", theme::bar_style().fg(theme::FG_DIMMED)),
        Span::styled("  e", theme::bar_style().fg(theme::FG_DEFAULT)),
        Span::styled(" edit", theme::bar_style().fg(theme::FG_DIMMED)),
        Span::styled("  Enter", theme::bar_style().fg(theme::FG_DEFAULT)),
        Span::styled(" launch", theme::bar_style().fg(theme::FG_DIMMED)),
        Span::styled("  i", theme::bar_style().fg(theme::FG_DEFAULT)),
        Span::styled(" import", theme::bar_style().fg(theme::FG_DIMMED)),
        Span::styled("  w", theme::bar_style().fg(theme::FG_DEFAULT)),
        Span::styled(" ~/.codex", theme::bar_style().fg(theme::FG_DIMMED)),
        Span::styled("  /", theme::bar_style().fg(theme::FG_DEFAULT)),
        Span::styled(" filter", theme::bar_style().fg(theme::FG_DIMMED)),
        Span::styled("  Tab", theme::bar_style().fg(theme::FG_DEFAULT)),
        Span::styled(" switch", theme::bar_style().fg(theme::FG_DIMMED)),
        Span::styled("  q", theme::bar_style().fg(theme::FG_DEFAULT)),
        Span::styled(" quit", theme::bar_style().fg(theme::FG_DIMMED)),
    ])
}
