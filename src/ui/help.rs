use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use super::theme;

pub fn render(frame: &mut Frame, scroll: u16) {
    let area = frame.area();

    let lines = help_lines(true);

    // Size the overlay to fit the content exactly, capped at the available area.
    // +2 accounts for the top and bottom border lines.
    let needed_height = lines.len() as u16 + 2;
    let overlay_height = needed_height.min(area.height.saturating_sub(2));
    let overlay_width = (area.width / 2).max(52).min(area.width);

    let x = (area.width.saturating_sub(overlay_width)) / 2;
    let y = (area.height.saturating_sub(overlay_height)) / 2;
    let overlay = Rect::new(x, y, overlay_width, overlay_height);

    frame.render_widget(Clear, overlay);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::FG_BORDER))
        .title(Span::styled(
            " keybinds  j/k to scroll ",
            Style::default()
                .fg(theme::FG_DEFAULT)
                .add_modifier(Modifier::BOLD),
        ))
        .style(Style::default().bg(theme::BG_SECTION));

    let paragraph = Paragraph::new(lines).scroll((scroll, 0)).block(block);
    frame.render_widget(paragraph, overlay);
}

pub fn help_lines(include_close_hint: bool) -> Vec<Line<'static>> {
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
            "  Accounts",
            Style::default()
                .fg(theme::FG_DEFAULT)
                .add_modifier(Modifier::BOLD),
        )),
        keybind_line("j / ↓", "Move selected profile"),
        keybind_line("k / ↑", "Move selected profile"),
        keybind_line("Enter", "Confirm pending action (write / delete)"),
        keybind_line("e", "Edit selected profile"),
        keybind_line("d", "Delete selected profile (confirms)"),
        keybind_line("w", "Write profile to home auth (confirms)"),
        keybind_line("i", "Import ~/.codex/auth.json (Codex)"),
        keybind_line("I", "Import Claude credentials"),
        keybind_line("R", "Refresh quota"),
        keybind_line("f", "Cycle provider filter"),
        keybind_line("/", "Start profile filter"),
        Line::from(""),
        keybind_line("?", "Toggle help"),
        keybind_line("Esc", "Dismiss overlay / clear filter"),
        keybind_line("q", "Quit"),
    ];

    if include_close_hint {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "  Press Esc or ? to close",
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

