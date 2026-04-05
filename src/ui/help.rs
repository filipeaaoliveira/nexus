use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::app::ActiveTab;

use super::theme;

pub fn render(frame: &mut Frame) {
    let area = frame.area();

    // Center the overlay: 50% width, 60% height
    let overlay = centered_rect(50, 60, area);

    frame.render_widget(Clear, overlay);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::FG_BORDER))
        .title(Span::styled(
            " keybinds ",
            Style::default()
                .fg(theme::FG_DEFAULT)
                .add_modifier(Modifier::BOLD),
        ))
        .style(Style::default().bg(theme::BG_SECTION));

    let lines = help_lines(ActiveTab::Overview, true);

    let paragraph = Paragraph::new(lines).block(block);
    frame.render_widget(paragraph, overlay);
}

pub fn help_lines(active_tab: ActiveTab, include_close_hint: bool) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::from(""),
        keybind_line("Tab", "Switch workspace"),
        Line::from(""),
        Line::from(Span::styled(
            "  Overview",
            Style::default()
                .fg(theme::FG_DEFAULT)
                .add_modifier(Modifier::BOLD),
        )),
        keybind_line("j / ↓", "Move selection down"),
        keybind_line("k / ↑", "Move selection up"),
        keybind_line("/", "Start filter"),
        keybind_line("s", "Cycle sort mode"),
        keybind_line("d", "Remove selected agent"),
        keybind_line("D", "Remove all exited agents"),
        keybind_line("r", "Rename selected agent"),
        Line::from(""),
        Line::from(Span::styled(
            "  Codex",
            Style::default()
                .fg(theme::FG_DEFAULT)
                .add_modifier(Modifier::BOLD),
        )),
        keybind_line("j / ↓", "Move selected profile"),
        keybind_line("k / ↑", "Move selected profile"),
        keybind_line("i", "Import current ~/.codex/auth.json"),
        keybind_line("/", "Start profile filter"),
        Line::from(""),
        keybind_line("?", "Toggle help"),
        keybind_line("Esc", "Dismiss overlay / clear filter"),
        keybind_line("q", "Quit"),
    ];

    if include_close_hint {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            match active_tab {
                ActiveTab::Overview => "  Press Esc or ? to close",
                ActiveTab::Codex => "  Press Esc or ? to close",
            },
            Style::default().fg(theme::FG_FADED),
        )));
    }

    lines
}

fn keybind_line(key: &str, desc: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("  {key:<10}"),
            Style::default()
                .fg(theme::FG_DEFAULT)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(desc.to_string(), Style::default().fg(theme::FG_DIMMED)),
    ])
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
