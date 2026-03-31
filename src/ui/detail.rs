use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::app::AppState;
use crate::types;

use super::theme;

pub fn render(frame: &mut Frame, state: &AppState, area: Rect) {
    let block = Block::default()
        .borders(Borders::BOTTOM)
        .border_style(theme::border_style())
        .title(Span::styled(
            " detail ",
            Style::default().fg(theme::FG_COMMENT),
        ))
        .style(Style::default().bg(theme::BG_MAIN));

    let visible = state.visible_agents();
    let agent = state.selected.and_then(|i| visible.get(i).copied());

    let lines = match agent {
        None => vec![Line::from(Span::styled(
            "  no agent selected",
            Style::default().fg(theme::FG_FADED),
        ))],
        Some(agent) => {
            let mut lines = Vec::new();

            // Status badge + PID
            lines.push(Line::from(vec![
                Span::styled("  ", Style::default()),
                Span::styled(
                    format!(" {} ", agent.status.as_str()),
                    theme::status_badge_style(agent.status),
                ),
                Span::styled(
                    format!("  PID: {}", agent.pid),
                    Style::default().fg(theme::FG_DIMMED),
                ),
            ]));

            // CWD
            lines.push(Line::from(vec![
                Span::styled("  cwd: ", Style::default().fg(theme::FG_COMMENT)),
                Span::styled(
                    agent.cwd.to_string_lossy().to_string(),
                    Style::default().fg(theme::COLOR_PATH),
                ),
            ]));

            // Branch
            if let Some(ref branch) = agent.branch {
                let wt = if agent.is_worktree { " (worktree)" } else { "" };
                lines.push(Line::from(vec![
                    Span::styled("  branch: ", Style::default().fg(theme::FG_COMMENT)),
                    Span::styled(
                        format!("{branch}{wt}"),
                        Style::default().fg(theme::COLOR_BRANCH),
                    ),
                ]));
            }

            // Activity
            if let Some(ref activity) = agent.activity {
                lines.push(Line::from(vec![
                    Span::styled("  activity: ", Style::default().fg(theme::FG_COMMENT)),
                    Span::styled(
                        activity.as_str().to_string(),
                        Style::default().fg(theme::FG_DEFAULT),
                    ),
                ]));
            }

            // Start time + idle
            lines.push(Line::from(vec![
                Span::styled("  started: ", Style::default().fg(theme::FG_COMMENT)),
                Span::styled(
                    agent.started_at.format("%H:%M:%S").to_string(),
                    Style::default().fg(theme::FG_DIMMED),
                ),
                Span::styled("  idle: ", Style::default().fg(theme::FG_COMMENT)),
                Span::styled(
                    types::format_duration_short(agent.idle_duration()),
                    Style::default().fg(theme::FG_DIMMED),
                ),
            ]));

            lines
        }
    };

    let paragraph = Paragraph::new(lines).block(block);
    frame.render_widget(paragraph, area);
}
