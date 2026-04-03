use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph};
use ratatui::Frame;

use crate::app::AppState;

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
    render_detail_placeholder(frame, state, main_chunks[1]);

    footer::render(frame, state, outer[2]);

    if state.codex_tab.session_picker_mode {
        render_session_picker(frame, state);
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
            let label = if is_selected {
                if is_active {
                    format!("> {} (active)", profile_id)
                } else {
                    format!("> {}", profile_id)
                }
            } else {
                if is_active {
                    format!("  {} (active)", profile_id)
                } else {
                    format!("  {}", profile_id)
                }
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
    list_state.select(state.codex_tab.selected_profile_index);

    frame.render_stateful_widget(list, area, &mut list_state);
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
        "  select a profile to inspect it here",
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
