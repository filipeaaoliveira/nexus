use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph};
use ratatui::Frame;

use crate::app::{AppState, EditField};
use crate::codex_profiles::profile::load_profile_metadata;

use super::{footer, header, theme, RenderData};

pub fn render(frame: &mut Frame, state: &AppState) {
    let render_data = RenderData::from_state(state);

    frame.render_widget(
        Block::default().style(Style::default().bg(theme::BG_MAIN)),
        frame.area(),
    );

    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(frame.area());

    header::render(frame, state, &render_data, outer[0]);

    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
        .split(outer[1]);

    render_profile_list(frame, state, main_chunks[0]);

    if state.codex_tab.view_mode {
        render_profile_details(frame, state, main_chunks[1]);
    } else {
        render_detail_placeholder(frame, state, main_chunks[1]);
    }

    footer::render(frame, state, outer[2]);

    if state.codex_tab.session_picker_mode {
        render_session_picker(frame, state);
    }

    if state.codex_tab.edit_mode {
        let edit_area = Rect::new(
            main_chunks[1].x + 5,
            main_chunks[1].y + 2,
            main_chunks[1].x + main_chunks[1].width - 5,
            main_chunks[1].y + 10,
        );
        render_edit_mode(frame, state, edit_area);
    }
}

fn render_profile_list(frame: &mut Frame, state: &AppState, area: Rect) {
    let block = Block::default()
        .borders(Borders::RIGHT)
        .border_style(theme::border_style())
        .title(Span::styled(
            " profiles ",
            Style::default().fg(theme::FG_COMMENT),
        ))
        .style(Style::default().bg(theme::BG_MAIN));

    let items: Vec<ListItem> = state
        .codex_tab
        .profile_ids
        .iter()
        .enumerate()
        .map(|(index, profile_id)| {
            let is_selected = state.codex_tab.selected_profile_index == Some(index);
            let is_active = state.active_codex_profile_id() == Some(profile_id.as_str());

            let profile_dir = state.config.codex_profiles_dir.join(profile_id);
            let (display_label, is_enabled) =
                if let Ok(metadata) = load_profile_metadata(&profile_dir) {
                    let label = metadata.label.unwrap_or_else(|| profile_id.clone());
                    (label, metadata.enabled)
                } else {
                    (profile_id.clone(), true)
                };

            let prefix = if is_selected { ">" } else { " " };
            let active_suffix = if is_active { " (active)" } else { "" };
            let enabled_suffix = if is_enabled { "" } else { " [disabled]" };

            let label = format!(
                "{} {}{}{}",
                prefix, display_label, active_suffix, enabled_suffix
            );

            let style = if is_enabled {
                Style::default().fg(theme::FG_DEFAULT)
            } else {
                Style::default().fg(theme::FG_FADED)
            };

            ListItem::new(Line::from(Span::styled(label, style)))
        })
        .collect();

    let list = List::new(items)
        .block(block)
        .highlight_style(theme::selected_style());

    let mut list_state = ListState::default();
    list_state.select(state.codex_tab.selected_profile_index);

    frame.render_stateful_widget(list, area, &mut list_state);
}

fn render_profile_details(frame: &mut Frame, state: &AppState, area: Rect) {
    let profile_id = match state.selected_codex_profile_id() {
        Some(id) => id,
        None => {
            render_detail_placeholder(frame, state, area);
            return;
        }
    };

    let profile_dir = state.config.codex_profiles_dir.join(profile_id);
    let metadata = load_profile_metadata(&profile_dir).ok();

    let block = Block::default()
        .borders(Borders::NONE)
        .title(Span::styled(
            " profile details ",
            Style::default().fg(theme::FG_COMMENT),
        ))
        .style(Style::default().bg(theme::BG_MAIN));

    let mut lines = Vec::new();

    let label = metadata
        .as_ref()
        .and_then(|m| m.label.clone())
        .unwrap_or_else(|| profile_id.to_string());
    lines.push(Line::from(vec![
        Span::styled("  label: ", Style::default().fg(theme::FG_COMMENT)),
        Span::styled(&label, Style::default().fg(theme::FG_DEFAULT)),
    ]));

    let email = metadata.as_ref().and_then(|m| m.email.clone());
    if let Some(ref email) = email {
        lines.push(Line::from(vec![
            Span::styled("  email: ", Style::default().fg(theme::FG_COMMENT)),
            Span::styled(email, Style::default().fg(theme::FG_DEFAULT)),
        ]));
    }

    let plan = metadata.as_ref().and_then(|m| m.plan.clone());
    if let Some(ref plan) = plan {
        lines.push(Line::from(vec![
            Span::styled("  plan: ", Style::default().fg(theme::FG_COMMENT)),
            Span::styled(plan, Style::default().fg(theme::FG_DEFAULT)),
        ]));
    }

    let enabled = metadata.as_ref().map(|m| m.enabled).unwrap_or(true);
    lines.push(Line::from(vec![
        Span::styled("  enabled: ", Style::default().fg(theme::FG_COMMENT)),
        Span::styled(
            if enabled { "yes" } else { "no" },
            if enabled {
                Style::default().fg(theme::STATUS_RUNNING)
            } else {
                Style::default().fg(theme::STATUS_EXITED)
            },
        ),
    ]));

    let notes = metadata.as_ref().and_then(|m| m.notes.clone());
    if let Some(ref notes) = notes {
        if !notes.is_empty() {
            lines.push(Line::from(Span::raw("")));
            lines.push(Line::from(vec![Span::styled(
                "  notes: ",
                Style::default().fg(theme::FG_COMMENT),
            )]));
            for note_line in notes.lines() {
                lines.push(Line::from(vec![
                    Span::styled("    ", Style::default()),
                    Span::styled(note_line, Style::default().fg(theme::FG_DEFAULT)),
                ]));
            }
        }
    }

    lines.push(Line::from(Span::raw("")));
    lines.push(Line::from(vec![
        Span::styled(" v/esc ", Style::default().fg(theme::FG_COMMENT)),
        Span::styled("close", Style::default().fg(theme::FG_DIMMED)),
    ]));

    frame.render_widget(Paragraph::new(lines).block(block), area);
}

fn render_detail_placeholder(frame: &mut Frame, state: &AppState, area: Rect) {
    let block = Block::default()
        .borders(Borders::NONE)
        .title(Span::styled(
            " profile details ",
            Style::default().fg(theme::FG_COMMENT),
        ))
        .style(Style::default().bg(theme::BG_MAIN));

    let mut lines = vec![Line::from(Span::styled(
        "  press v to view profile details",
        Style::default().fg(theme::FG_FADED),
    ))];

    if state.codex_tab.filter_mode {
        lines.push(Line::from(vec![
            Span::styled("  filter: ", Style::default().fg(theme::FG_COMMENT)),
            Span::styled(
                &state.codex_tab.filter_input,
                Style::default().fg(theme::FG_DEFAULT),
            ),
            Span::styled("_", Style::default().fg(theme::STATUS_WAITING)),
        ]));
    }

    if let Some(message) = state.codex_tab.status_message.as_ref() {
        lines.push(Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled(message, Style::default().fg(theme::STATUS_WAITING)),
        ]));
    }

    frame.render_widget(Paragraph::new(lines).block(block), area);
}

fn render_session_picker(frame: &mut Frame, state: &AppState) {
    let area = frame.area();
    let picker_width = 40;
    let picker_height = 5.max((state.codex_tab.profile_ids.len() + 3).min(20));

    let x = (area.width as usize).saturating_sub(picker_width) / 2 + area.x as usize;
    let y = (area.height as usize).saturating_sub(picker_height) / 2 + area.y as usize;

    let picker_area = Rect::new(
        x as u16,
        y as u16,
        (x + picker_width) as u16,
        (y + picker_height) as u16,
    );

    frame.render_widget(Clear, picker_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::selected_style())
        .title(Span::styled(
            " restart session ",
            Style::default().fg(theme::FG_DEFAULT),
        ))
        .style(Style::default().bg(theme::BG_MAIN));

    let items: Vec<ListItem> = state
        .codex_tab
        .profile_ids
        .iter()
        .enumerate()
        .map(|(index, profile_id)| {
            let is_selected = state.codex_tab.session_picker_selected == Some(index);
            let label = if is_selected {
                format!("> {}", profile_id)
            } else {
                format!("  {}", profile_id)
            };
            ListItem::new(Line::from(Span::styled(
                label,
                Style::default().fg(theme::FG_DEFAULT),
            )))
        })
        .collect();

    let list = List::new(items)
        .block(block)
        .highlight_style(theme::selected_style());

    let mut list_state = ListState::default();
    list_state.select(state.codex_tab.session_picker_selected);

    frame.render_stateful_widget(list, picker_area, &mut list_state);
}

fn render_edit_mode(frame: &mut Frame, state: &AppState, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::border_style())
        .title(Span::styled(
            " Edit Profile ",
            Style::default().fg(theme::FG_COMMENT),
        ))
        .style(Style::default().bg(theme::BG_MAIN));

    let mut lines = Vec::new();

    let label_prefix = if state.codex_tab.edit_field == EditField::Label {
        "> "
    } else {
        "  "
    };
    lines.push(Line::from(vec![
        Span::raw(label_prefix),
        Span::styled("label: ", Style::default().fg(theme::FG_COMMENT)),
        Span::styled(
            &state.codex_tab.edit_label,
            Style::default().fg(theme::FG_DEFAULT),
        ),
    ]));

    let notes_prefix = if state.codex_tab.edit_field == EditField::Notes {
        "> "
    } else {
        "  "
    };
    lines.push(Line::from(vec![
        Span::raw(notes_prefix),
        Span::styled("notes: ", Style::default().fg(theme::FG_COMMENT)),
        Span::styled(
            &state.codex_tab.edit_notes,
            Style::default().fg(theme::FG_DEFAULT),
        ),
    ]));

    let enabled_prefix = if state.codex_tab.edit_field == EditField::Enabled {
        "> "
    } else {
        "  "
    };
    let enabled_status = if state.codex_tab.edit_enabled {
        "enabled"
    } else {
        "disabled"
    };
    let enabled_style = if state.codex_tab.edit_enabled {
        Style::default().fg(theme::STATUS_RUNNING)
    } else {
        Style::default().fg(theme::STATUS_EXITED)
    };
    lines.push(Line::from(vec![
        Span::raw(enabled_prefix),
        Span::styled("enabled: ", Style::default().fg(theme::FG_COMMENT)),
        Span::styled(enabled_status, enabled_style),
    ]));

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

    frame.render_widget(Paragraph::new(lines).block(block), area);
}
