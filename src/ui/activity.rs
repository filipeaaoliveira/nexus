use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::app::AppState;
use crate::types::{self, ActivityEventKind};

use super::theme;

pub fn render(frame: &mut Frame, state: &AppState, area: Rect) {
    let block = Block::default()
        .borders(Borders::NONE)
        .title(Span::styled(
            " activity ",
            Style::default().fg(theme::FG_COMMENT),
        ))
        .style(Style::default().bg(theme::BG_MAIN));

    let visible = state.visible_agents();
    let agent = state.selected.and_then(|i| visible.get(i).copied());

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let Some(agent) = agent else {
        return;
    };

    // Split area: subagents top, events bottom
    let has_subagents = !agent.subagents.is_empty();
    let constraints = if has_subagents {
        vec![
            Constraint::Length((agent.subagents.len() as u16 + 2).min(inner.height / 2)),
            Constraint::Min(0),
        ]
    } else {
        vec![Constraint::Length(0), Constraint::Min(0)]
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(inner);

    // Subagents section
    if has_subagents {
        let mut sub_lines = Vec::new();
        sub_lines.push(Line::from(Span::styled(
            "  subagents",
            Style::default().fg(theme::FG_COMMENT),
        )));

        let count = agent.subagents.len();
        for (i, sub) in agent.subagents.iter().enumerate() {
            let connector = if i == count - 1 { "└─" } else { "├─" };
            let elapsed = if let Some(ended) = sub.ended_at {
                let d = ended - sub.started_at;
                types::format_duration(d)
            } else {
                let d = chrono::Local::now() - sub.started_at;
                types::format_duration(d)
            };

            sub_lines.push(Line::from(vec![
                Span::styled(
                    format!("  {connector} "),
                    Style::default().fg(theme::FG_FADED),
                ),
                Span::styled(&sub.name, Style::default().fg(theme::COLOR_SUBAGENT)),
                Span::styled("  ", Style::default()),
                Span::styled(
                    sub.status.as_str(),
                    Style::default().fg(theme::status_color(sub.status)),
                ),
                Span::styled(
                    format!("  {elapsed}"),
                    Style::default().fg(theme::FG_DIMMED),
                ),
            ]));
        }

        frame.render_widget(Paragraph::new(sub_lines), chunks[0]);
    }

    // Events section
    let mut event_lines = Vec::new();
    event_lines.push(Line::from(Span::styled(
        "  events",
        Style::default().fg(theme::FG_COMMENT),
    )));

    if agent.recent_events.is_empty() {
        event_lines.push(Line::from(Span::styled(
            "  no events yet",
            Style::default().fg(theme::FG_FADED),
        )));
    } else {
        for event in agent.recent_events.iter() {
            let ts = event.timestamp.format("%H:%M:%S").to_string();
            let color = match event.kind {
                ActivityEventKind::ToolCall => theme::COLOR_TOOL_EVENT,
                ActivityEventKind::SubagentSpawned | ActivityEventKind::SubagentCompleted => {
                    theme::COLOR_SUB_EVENT
                }
                ActivityEventKind::StatusChanged => theme::STATUS_WAITING,
                ActivityEventKind::Message => theme::COLOR_MSG_EVENT,
            };

            event_lines.push(Line::from(vec![
                Span::styled(format!("  {ts} "), Style::default().fg(theme::FG_FADED)),
                Span::styled(&event.description, Style::default().fg(color)),
            ]));
        }
    }

    frame.render_widget(Paragraph::new(event_lines), chunks[1]);
}
