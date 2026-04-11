use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::types::{self, AgentTool, TodoStatus};

use super::{theme, RenderData};

pub fn render(frame: &mut Frame, render_data: &RenderData<'_>, area: Rect) {
    let block = Block::default()
        .borders(Borders::BOTTOM)
        .border_style(theme::border_style())
        .title(Span::styled(
            " detail ",
            Style::default().fg(theme::FG_COMMENT),
        ))
        .style(Style::default().bg(theme::BG_MAIN));

    let lines = match render_data.selected_agent {
        None => vec![Line::from(Span::styled(
            "  no agent selected",
            Style::default().fg(theme::FG_FADED),
        ))],
        Some(agent) => {
            let mut lines = Vec::new();
            let (status_label, activity_label) = if agent.id.tool() == AgentTool::Claude {
                ("  observed status: ", "  observed activity: ")
            } else {
                ("  runtime status: ", "  runtime activity: ")
            };
            let normalized_text = match &agent.normalized.reason {
                Some(reason) => {
                    format!("{} · {}", agent.normalized.status.as_str(), reason.as_str())
                }
                None => agent.normalized.status.as_str().to_string(),
            };

            // Normalized state badge + PID
            lines.push(Line::from(vec![
                Span::styled("  ", Style::default()),
                Span::styled(
                    format!(" {} ", normalized_text),
                    normalized_status_badge_style(agent.normalized.status),
                ),
                Span::styled(
                    format!(
                        "  PID: {}",
                        agent
                            .pid
                            .map(|pid| pid.to_string())
                            .unwrap_or_else(|| "-".to_string())
                    ),
                    Style::default().fg(theme::FG_DIMMED),
                ),
            ]));

            // Tool-native diagnostics
            lines.push(Line::from(vec![
                Span::styled(status_label, Style::default().fg(theme::FG_COMMENT)),
                Span::styled(
                    agent.status.as_str().to_string(),
                    Style::default().fg(theme::status_color(agent.status)),
                ),
                Span::styled(activity_label, Style::default().fg(theme::FG_COMMENT)),
                Span::styled(
                    agent
                        .activity
                        .map(|activity| activity.as_str().to_string())
                        .unwrap_or_else(|| "idle".to_string()),
                    Style::default().fg(theme::FG_DEFAULT),
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

            // Todos section
            if !agent.todos.is_empty() {
                // Blank separator line
                lines.push(Line::from(""));

                // Progress header: "Todos (completed/total)"
                let completed = agent
                    .todos
                    .iter()
                    .filter(|t| t.status == TodoStatus::Completed)
                    .count();
                let total = agent.todos.len();
                lines.push(Line::from(Span::styled(
                    format!("  Todos ({completed}/{total})"),
                    Style::default().fg(theme::COLOR_TODO_HEADER),
                )));

                // Individual todo items
                for todo in &agent.todos {
                    let (symbol, symbol_color, content_color) = match todo.status {
                        TodoStatus::InProgress => {
                            ("\u{25CF}", theme::COLOR_TODO_IN_PROGRESS, theme::FG_DEFAULT)
                        }
                        TodoStatus::Pending => {
                            ("\u{25CB}", theme::COLOR_TODO_PENDING, theme::FG_DIMMED)
                        }
                        TodoStatus::Completed => {
                            ("\u{2713}", theme::COLOR_TODO_DONE, theme::COLOR_TODO_DONE)
                        }
                        TodoStatus::Cancelled => {
                            ("\u{2298}", theme::COLOR_TODO_DONE, theme::COLOR_TODO_DONE)
                        }
                    };
                    lines.push(Line::from(vec![
                        Span::styled(format!("  {symbol} "), Style::default().fg(symbol_color)),
                        Span::styled(todo.content.clone(), Style::default().fg(content_color)),
                    ]));
                }
            }

            lines
        }
    };

    let paragraph = Paragraph::new(lines).block(block);
    frame.render_widget(paragraph, area);
}

fn normalized_status_badge_style(status: crate::types::NormalizedStatus) -> Style {
    let color = normalized_status_color(status);
    Style::default()
        .fg(theme::BG_HEADER)
        .bg(color)
        .add_modifier(Modifier::BOLD)
}

fn normalized_status_color(status: crate::types::NormalizedStatus) -> Color {
    match status {
        crate::types::NormalizedStatus::Working => theme::STATUS_RUNNING,
        crate::types::NormalizedStatus::NeedsInput => theme::STATUS_WAITING,
        crate::types::NormalizedStatus::Blocked => theme::STATUS_ERROR,
        crate::types::NormalizedStatus::Done => theme::STATUS_DONE,
        crate::types::NormalizedStatus::Unknown => theme::FG_DIMMED,
    }
}
