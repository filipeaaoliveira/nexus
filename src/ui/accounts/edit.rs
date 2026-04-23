use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::app::state::AccountKey;
use crate::app::{AccountsEditField, AccountsMode, AppState};
use crate::ui::theme;

pub fn render(frame: &mut Frame, state: &AppState, area: Rect) {
    let AccountsMode::Editing { draft, field } = state.root().accounts().mode() else {
        return;
    };

    frame.render_widget(Clear, area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::selected_style())
        .title(Span::styled(
            " Edit Profile ",
            Style::default()
                .fg(theme::FG_DEFAULT)
                .add_modifier(Modifier::BOLD),
        ))
        .style(Style::default().bg(theme::BG_DIALOG));

    let mut lines = Vec::new();

    lines.push(edit_mode_row(
        "label",
        &draft.label,
        *field == AccountsEditField::Label,
        Some(theme::STATUS_WAITING),
    ));

    // Show read-only profile context (email/plan) from the authoritative profile store
    if let Some(entry) = state.root().accounts().selected_entry() {
        let key = AccountKey::for_entry(entry);
        if let Some(profile) = state.root().accounts().profile_by_key(&key) {
            if let Some(email) = profile.email() {
                lines.push(Line::from(vec![
                    Span::raw("  "),
                    Span::styled("email: ", Style::default().fg(theme::FG_COMMENT)),
                    Span::styled(email.to_string(), Style::default().fg(theme::FG_DEFAULT)),
                ]));
            }
            if let Some(plan) = profile.plan() {
                lines.push(Line::from(vec![
                    Span::raw("  "),
                    Span::styled("plan: ", Style::default().fg(theme::FG_COMMENT)),
                    Span::styled(plan.to_string(), Style::default().fg(theme::FG_DEFAULT)),
                ]));
            }
        }
    }

    lines.push(edit_mode_row(
        "notes",
        &draft.notes,
        *field == AccountsEditField::Notes,
        Some(theme::STATUS_WAITING),
    ));

    let enabled_status = if draft.enabled { "enabled" } else { "disabled" };
    lines.push(edit_mode_row(
        "enabled",
        enabled_status,
        *field == AccountsEditField::Enabled,
        Some(if draft.enabled {
            theme::STATUS_RUNNING
        } else {
            theme::STATUS_ERROR
        }),
    ));

    lines.push(Line::from(Span::raw("")));
    lines.push(Line::from(vec![
        Span::styled("Enter", Style::default().fg(theme::FG_COMMENT)),
        Span::raw(" save | "),
        Span::styled("Esc", Style::default().fg(theme::FG_COMMENT)),
        Span::raw(" cancel | "),
        Span::styled("Space", Style::default().fg(theme::FG_COMMENT)),
        Span::raw(" toggle | "),
        Span::styled("Tab", Style::default().fg(theme::FG_COMMENT)),
        Span::raw(" next field"),
    ]));

    frame.render_widget(
        Paragraph::new(lines)
            .block(block)
            .style(Style::default().bg(theme::BG_DIALOG)),
        area,
    );
}

fn edit_mode_row(
    label: &str,
    value: &str,
    active: bool,
    value_color: Option<ratatui::style::Color>,
) -> Line<'static> {
    let row_bg = if active {
        theme::BG_SELECTED
    } else {
        theme::BG_DIALOG
    };
    let prefix_style = if active {
        Style::default()
            .fg(theme::STATUS_WAITING)
            .bg(row_bg)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::FG_DIMMED).bg(row_bg)
    };
    let label_style = Style::default().fg(theme::FG_COMMENT).bg(row_bg);
    let value_style = Style::default()
        .fg(value_color.unwrap_or(theme::FG_DEFAULT))
        .bg(row_bg);

    let mut spans = vec![
        Span::styled(if active { "▶ " } else { "  " }, prefix_style),
        Span::styled(format!("{label}: "), label_style),
        Span::styled(value.to_string(), value_style),
    ];

    if active {
        spans.push(Span::styled(
            "█",
            Style::default()
                .fg(theme::BG_HEADER)
                .bg(theme::STATUS_WAITING)
                .add_modifier(Modifier::BOLD),
        ));
    }

    Line::from(spans)
}

#[cfg(test)]
mod tests {
    use ratatui::backend::TestBackend;
    use ratatui::layout::Rect;
    use ratatui::Terminal;

    use crate::app::{AppState, EditableProfile};
    use crate::services::runtime::RuntimeSession;

    use super::render;

    fn render_edit_popup(state: &AppState, area: Rect) -> ratatui::buffer::Buffer {
        let backend = TestBackend::new(area.width, area.height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                render(frame, state, area);
            })
            .unwrap();
        terminal.backend().buffer().clone()
    }

    fn buffer_text(buffer: &ratatui::buffer::Buffer) -> String {
        buffer
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<Vec<_>>()
            .join("")
    }

    #[tokio::test]
    async fn edit_popup_shows_cursor_for_active_field() {
        let mut session = RuntimeSession::new(crate::config::Config::default(), AppState::new());
        session
            .run_accounts_reducer_action(crate::app::AccountsAction::StartEditing(
                EditableProfile {
                    label: "Personal".to_string(),
                    notes: "Primary".to_string(),
                    enabled: true,
                },
            ))
            .await;

        let buffer = render_edit_popup(session.app(), Rect::new(0, 0, 48, 10));
        let text = buffer_text(&buffer);

        assert!(
            text.contains("Personal█"),
            "active field should show a cursor-like marker"
        );
    }
}
