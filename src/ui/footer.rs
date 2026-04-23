use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::{AccountsMode, AppState};

use super::theme;

pub fn render(frame: &mut Frame, state: &AppState, area: Rect) {
    if let crate::app::OverviewMode::Renaming { input, .. } = state.root().overview().mode() {
        let line = Line::from(vec![
            Span::styled(" rename: ", theme::bar_style().fg(theme::STATUS_WAITING)),
            Span::styled(input.clone(), theme::bar_style().fg(theme::FG_DEFAULT)),
            Span::styled("_", theme::bar_style().fg(theme::STATUS_WAITING)),
        ]);
        let footer = Paragraph::new(line).style(theme::bar_style());
        frame.render_widget(footer, area);
        return;
    }

    if let crate::app::OverviewMode::Filtering { input } = state.root().overview().mode() {
        let line = Line::from(vec![
            Span::styled(" / ", theme::bar_style().fg(theme::STATUS_WAITING)),
            Span::styled(input.clone(), theme::bar_style().fg(theme::FG_DEFAULT)),
            Span::styled("_", theme::bar_style().fg(theme::STATUS_WAITING)),
        ]);
        let footer = Paragraph::new(line).style(theme::bar_style());
        frame.render_widget(footer, area);
        return;
    }

    if let AccountsMode::Filtering { input } = state.root().accounts().mode() {
        let line = Line::from(vec![
            Span::styled(" / ", theme::bar_style().fg(theme::STATUS_WAITING)),
            Span::styled(input.clone(), theme::bar_style().fg(theme::FG_DEFAULT)),
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

#[cfg(test)]
pub fn shortcut_text(state: &AppState) -> String {
    shortcut_line(state)
        .spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect()
}

fn shortcut_line(state: &AppState) -> Line<'static> {
    match state.active_tab() {
        crate::app::Workspace::Overview => overview_shortcut_line(state),
        crate::app::Workspace::Accounts => accounts_shortcut_line(state),
    }
}

fn overview_shortcut_line(state: &AppState) -> Line<'static> {
    let sort_label = state.root().overview().sort_mode().as_str();
    Line::from(vec![
        Span::styled(" j/k", theme::bar_style().fg(theme::FG_DEFAULT)),
        Span::styled(" navigate", theme::bar_style().fg(theme::FG_DIMMED)),
        Span::styled("  Tab", theme::bar_style().fg(theme::FG_DEFAULT)),
        Span::styled(" switch", theme::bar_style().fg(theme::FG_DIMMED)),
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

fn accounts_shortcut_line(state: &AppState) -> Line<'static> {
    if matches!(
        state.root().accounts().mode(),
        AccountsMode::Confirming { .. }
    ) {
        return Line::from(vec![
            Span::styled(" Enter", theme::bar_style().fg(theme::FG_DEFAULT)),
            Span::styled(" confirm", theme::bar_style().fg(theme::FG_DIMMED)),
            Span::styled("  Esc", theme::bar_style().fg(theme::FG_DEFAULT)),
            Span::styled(" cancel", theme::bar_style().fg(theme::FG_DIMMED)),
        ]);
    }
    Line::from(vec![
        Span::styled(" j/k", theme::bar_style().fg(theme::FG_DEFAULT)),
        Span::styled(" accounts", theme::bar_style().fg(theme::FG_DIMMED)),
        Span::styled("  e", theme::bar_style().fg(theme::FG_DEFAULT)),
        Span::styled(" edit", theme::bar_style().fg(theme::FG_DIMMED)),
        Span::styled("  d", theme::bar_style().fg(theme::FG_DEFAULT)),
        Span::styled(" delete", theme::bar_style().fg(theme::FG_DIMMED)),
        Span::styled("  i", theme::bar_style().fg(theme::FG_DEFAULT)),
        Span::styled(" import codex", theme::bar_style().fg(theme::FG_DIMMED)),
        Span::styled("  I", theme::bar_style().fg(theme::FG_DEFAULT)),
        Span::styled(" import claude", theme::bar_style().fg(theme::FG_DIMMED)),
        Span::styled("  w", theme::bar_style().fg(theme::FG_DEFAULT)),
        Span::styled(" write", theme::bar_style().fg(theme::FG_DIMMED)),
        Span::styled("  R", theme::bar_style().fg(theme::FG_DEFAULT)),
        Span::styled(" refresh", theme::bar_style().fg(theme::FG_DIMMED)),
        Span::styled("  f", theme::bar_style().fg(theme::FG_DEFAULT)),
        Span::styled(" filter", theme::bar_style().fg(theme::FG_DIMMED)),
        Span::styled("  /", theme::bar_style().fg(theme::FG_DEFAULT)),
        Span::styled(" search", theme::bar_style().fg(theme::FG_DIMMED)),
        Span::styled("  ?", theme::bar_style().fg(theme::FG_DEFAULT)),
        Span::styled(" help", theme::bar_style().fg(theme::FG_DIMMED)),
        Span::styled("  Tab", theme::bar_style().fg(theme::FG_DEFAULT)),
        Span::styled(" switch", theme::bar_style().fg(theme::FG_DIMMED)),
        Span::styled("  q", theme::bar_style().fg(theme::FG_DEFAULT)),
        Span::styled(" quit", theme::bar_style().fg(theme::FG_DIMMED)),
    ])
}
