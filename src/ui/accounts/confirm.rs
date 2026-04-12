use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::app::{AppState, ConfirmKind};
use crate::ui::theme;

pub fn render(frame: &mut Frame, state: &AppState, confirm: &ConfirmKind) {
    let area = frame.area();
    let dialog_width = 52u16;
    let dialog_height = 5u16;
    let x = area.width.saturating_sub(dialog_width) / 2 + area.x;
    let y = area.height.saturating_sub(dialog_height) / 2 + area.y;
    let dialog_area = Rect::new(
        x,
        y,
        dialog_width.min(area.width),
        dialog_height.min(area.height),
    );

    frame.render_widget(Clear, dialog_area);

    let profile_id = state.selected_account().map(|(_, id)| id).unwrap_or("?");
    let (title, body) = match confirm {
        ConfirmKind::DeleteProfile => (
            " Delete Profile ",
            format!(" Delete {}? This cannot be undone.", profile_id),
        ),
        ConfirmKind::WriteToHome => (
            " Write to Home ",
            format!(" Overwrite home auth with {}?", profile_id),
        ),
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::selected_style())
        .title(Span::styled(title, Style::default().fg(theme::FG_DEFAULT)))
        .style(Style::default().bg(theme::BG_MAIN));

    let lines = vec![
        Line::from(Span::styled(body, Style::default().fg(theme::FG_DEFAULT))),
        Line::from(Span::raw("")),
        Line::from(vec![
            Span::styled("  Enter", Style::default().fg(theme::FG_DEFAULT)),
            Span::styled(" confirm  ", Style::default().fg(theme::FG_DIMMED)),
            Span::styled("Esc", Style::default().fg(theme::FG_DEFAULT)),
            Span::styled(" cancel", Style::default().fg(theme::FG_DIMMED)),
        ]),
    ];

    frame.render_widget(Paragraph::new(lines).block(block), dialog_area);
}
