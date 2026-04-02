use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::AppState;

use super::{theme, RenderData};

pub fn render(frame: &mut Frame, state: &AppState, render_data: &RenderData<'_>, area: Rect) {
    let total = render_data.header.total;
    let running = render_data.header.running;
    let waiting = render_data.header.waiting;
    let error = render_data.header.error;
    let done = render_data.header.done;
    let exited = render_data.header.exited;
    let subagent_count = render_data.header.subagent_count;

    let spans = vec![
        Span::styled(" ai-overview ", theme::bar_style().fg(theme::FG_DEFAULT)),
        Span::styled("  ", theme::bar_style()),
        Span::styled(
            format!("{total} agents"),
            theme::bar_style().fg(theme::FG_DIMMED),
        ),
        Span::styled("  ", theme::bar_style()),
        Span::styled(
            format!("●{running}"),
            theme::bar_style().fg(theme::STATUS_RUNNING),
        ),
        Span::styled(" ", theme::bar_style()),
        Span::styled(
            format!("◌{waiting}"),
            theme::bar_style().fg(theme::STATUS_WAITING),
        ),
        Span::styled(" ", theme::bar_style()),
        Span::styled(
            format!("✕{error}"),
            theme::bar_style().fg(theme::STATUS_ERROR),
        ),
        Span::styled(" ", theme::bar_style()),
        Span::styled(
            format!("✓{done}"),
            theme::bar_style().fg(theme::STATUS_DONE),
        ),
        Span::styled(" ", theme::bar_style()),
        Span::styled(
            format!("⏏{exited}"),
            theme::bar_style().fg(theme::STATUS_EXITED),
        ),
        Span::styled("  ", theme::bar_style()),
        Span::styled(
            format!("{subagent_count} subagents"),
            theme::bar_style().fg(theme::COLOR_SUBAGENT),
        ),
    ];

    let right_spans = vec![Span::styled(
        format!("poll: {}s ", state.config.poll_interval_secs),
        theme::bar_style().fg(theme::FG_DIMMED),
    )];

    let header_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(0), Constraint::Length(12)])
        .split(area);

    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().bg(theme::BG_HEADER)),
        header_layout[0],
    );
    frame.render_widget(
        Paragraph::new(Line::from(right_spans))
            .style(Style::default().bg(theme::BG_HEADER))
            .alignment(Alignment::Right),
        header_layout[1],
    );
}
