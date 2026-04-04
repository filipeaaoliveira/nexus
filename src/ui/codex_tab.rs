use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph};
use ratatui::Frame;

use crate::app::{AppState, EditField};
use crate::codex_profiles::profile::{
    detect_live_profile_id, extract_auth_metadata, load_profile_metadata,
};
use serde_json::Value;

use super::{footer, header, theme, RenderData};

/// Returns (bar_string, fill_color) for a quota percentage.
/// `bar_width` is the number of block characters to render.
fn quota_bar_spans(pct: u8, bar_width: usize) -> (String, ratatui::style::Color) {
    let filled = ((pct as usize).min(100) * bar_width) / 100;
    let empty = bar_width - filled;
    let bar = format!("{}{}", "█".repeat(filled), "░".repeat(empty));
    let color = if pct <= 50 {
        theme::STATUS_RUNNING
    } else if pct <= 80 {
        theme::STATUS_WAITING
    } else {
        theme::STATUS_ERROR
    };
    (bar, color)
}

/// Returns a human-readable duration string like "2h 14m" or "3d 6h".
fn format_reset_duration(reset_at: chrono::DateTime<chrono::Utc>) -> String {
    let now = chrono::Utc::now();
    let secs = (reset_at - now).num_seconds().max(0);
    let total_minutes = secs / 60;
    let total_hours = total_minutes / 60;
    if total_hours >= 24 {
        let days = total_hours / 24;
        let hours = total_hours % 24;
        format!("{}d {}h", days, hours)
    } else {
        let hours = total_hours;
        let minutes = total_minutes % 60;
        format!("{}h {}m", hours, minutes)
    }
}

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

    render_profile_details(frame, state, main_chunks[1]);

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

    if state.codex_tab.quota_edit_mode {
        let edit_area = Rect::new(
            main_chunks[1].x + 5,
            main_chunks[1].y + 2,
            main_chunks[1].x + main_chunks[1].width - 5,
            main_chunks[1].y + 10,
        );
        render_quota_edit_mode(frame, state, edit_area);
    }
}

fn render_profile_list(frame: &mut Frame, state: &AppState, area: Rect) {
    let live_profile_id = detect_live_profile_id(
        &state.config.codex_profiles_dir,
        &state.config.codex_data_dir.join("auth.json"),
    );

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

            // Get quota indicator if available
            let quota_indicator = if let Some(ref cache) = state.codex_tab.quota_cache {
                if let Some(quota) = cache.by_profile.get(profile_id) {
                    let five_h = match quota.five_hour_state {
                        crate::codex_profiles::quota::LimitState::Hit => "✗",
                        crate::codex_profiles::quota::LimitState::Available => "✓",
                        crate::codex_profiles::quota::LimitState::Unknown => "?",
                    };
                    let wk = match quota.weekly_state {
                        crate::codex_profiles::quota::LimitState::Hit => "✗",
                        crate::codex_profiles::quota::LimitState::Available => "✓",
                        crate::codex_profiles::quota::LimitState::Unknown => "?",
                    };
                    Some(format!("[{}/{}]", five_h, wk))
                } else {
                    None
                }
            } else {
                None
            };

            let prefix = if is_selected { ">" } else { " " };
            let live_marker = if live_profile_id.as_deref() == Some(profile_id.as_str()) {
                " *"
            } else {
                ""
            };
            let active_suffix = if is_active { " (active)" } else { "" };
            let enabled_suffix = if is_enabled { "" } else { " [disabled]" };
            let quota_str = quota_indicator.unwrap_or_default();

            let label = format!(
                "{} {}{}{}{}{}",
                prefix, display_label, quota_str, live_marker, active_suffix, enabled_suffix
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

    let auth_email;
    let auth_plan;
    let auth_path = profile_dir.join("auth.json");
    if let Ok(auth_content) = std::fs::read_to_string(&auth_path) {
        if let Ok(auth_json) = serde_json::from_str::<Value>(&auth_content) {
            let auth_meta = extract_auth_metadata(&auth_json);
            auth_email = auth_meta.email;
            auth_plan = auth_meta.plan;
        } else {
            auth_email = None;
            auth_plan = None;
        }
    } else {
        auth_email = None;
        auth_plan = None;
    }

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

    let email = metadata
        .as_ref()
        .and_then(|m| m.email.clone())
        .or(auth_email);
    if let Some(ref email) = email {
        lines.push(Line::from(vec![
            Span::styled("  email: ", Style::default().fg(theme::FG_COMMENT)),
            Span::styled(email, Style::default().fg(theme::FG_DEFAULT)),
        ]));
    }

    let plan = metadata.as_ref().and_then(|m| m.plan.clone()).or(auth_plan);
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

    if let Some(ref quota) = state.codex_tab.quota {
        lines.push(Line::from(Span::raw("")));

        // Helper closure: build one quota row, using a progress bar if pct is known,
        // otherwise falling back to the Hit/Available/? text label.
        let quota_row = |label: &'static str,
                         pct: Option<u8>,
                         state: &crate::codex_profiles::quota::LimitState,
                         reset_at: Option<chrono::DateTime<chrono::Utc>>|
         -> Line {
            if let Some(pct) = pct {
                let (bar, bar_color) = quota_bar_spans(pct, 20);
                let mut spans = vec![
                    Span::styled(label, Style::default().fg(theme::FG_COMMENT)),
                    Span::styled(bar, Style::default().fg(bar_color)),
                    Span::styled(format!(" {}%", pct), Style::default().fg(theme::FG_DEFAULT)),
                ];
                if let Some(reset) = reset_at {
                    spans.push(Span::styled(
                        format!("  resets in {}", format_reset_duration(reset)),
                        Style::default().fg(theme::FG_COMMENT),
                    ));
                }
                Line::from(spans)
            } else {
                // Fallback: no percentage data (old cache or CLI-only snapshot).
                let text_label = match state {
                    crate::codex_profiles::quota::LimitState::Hit => "HIT",
                    crate::codex_profiles::quota::LimitState::Available => "ok",
                    crate::codex_profiles::quota::LimitState::Unknown => "?",
                };
                let text_color = match state {
                    crate::codex_profiles::quota::LimitState::Hit => theme::STATUS_ERROR,
                    crate::codex_profiles::quota::LimitState::Available => theme::STATUS_RUNNING,
                    crate::codex_profiles::quota::LimitState::Unknown => theme::FG_COMMENT,
                };
                let mut spans = vec![
                    Span::styled(label, Style::default().fg(theme::FG_COMMENT)),
                    Span::styled(text_label, Style::default().fg(text_color)),
                ];
                if let Some(reset) = reset_at {
                    let reset_local = chrono::DateTime::<chrono::Local>::from(reset);
                    spans.push(Span::styled(
                        format!("  resets {}", reset_local.format("%a %H:%M")),
                        Style::default().fg(theme::FG_COMMENT),
                    ));
                }
                Line::from(spans)
            }
        };

        lines.push(quota_row(
            "  quota (5h):  ",
            quota.five_hour_used_pct,
            &quota.five_hour_state,
            quota.five_hour_reset_at,
        ));
        lines.push(quota_row(
            "  quota (7d):  ",
            quota.weekly_used_pct,
            &quota.weekly_state,
            quota.weekly_reset_at,
        ));

        // Show when quota was last captured.
        let captured_local = chrono::DateTime::<chrono::Local>::from(quota.captured_at);
        lines.push(Line::from(Span::styled(
            format!("  as of {}", captured_local.format("%H:%M")),
            Style::default().fg(theme::FG_COMMENT),
        )));
    }

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

    let mut lines = Vec::new();

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

    let is_sessions = state.codex_tab.session_picker_sessions.is_some();
    let items: &[String] = if is_sessions {
        state.codex_tab.session_picker_sessions.as_ref().unwrap()
    } else {
        &state.codex_tab.profile_ids
    };

    let picker_height = 5.max((items.len() + 3).min(20));

    let x = (area.width as usize).saturating_sub(picker_width) / 2 + area.x as usize;
    let y = (area.height as usize).saturating_sub(picker_height) / 2 + area.y as usize;

    let picker_area = Rect::new(
        x as u16,
        y as u16,
        (x + picker_width) as u16,
        (y + picker_height) as u16,
    );

    frame.render_widget(Clear, picker_area);

    let title = if is_sessions {
        " resume session "
    } else {
        " select profile "
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::selected_style())
        .title(Span::styled(title, Style::default().fg(theme::FG_DEFAULT)))
        .style(Style::default().bg(theme::BG_MAIN));

    let list_items: Vec<ListItem> = items
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let is_selected = state.codex_tab.session_picker_selected == Some(index);
            let label = if is_selected {
                format!("> {}", item)
            } else {
                format!("  {}", item)
            };
            ListItem::new(Line::from(Span::styled(
                label,
                Style::default().fg(theme::FG_DEFAULT),
            )))
        })
        .collect();

    let list = List::new(list_items)
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

    if let Some(ref email) = state.codex_tab.edit_email {
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled("email: ", Style::default().fg(theme::FG_COMMENT)),
            Span::styled(email, Style::default().fg(theme::FG_DEFAULT)),
        ]));
    }

    if let Some(ref plan) = state.codex_tab.edit_plan {
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled("plan: ", Style::default().fg(theme::FG_COMMENT)),
            Span::styled(plan, Style::default().fg(theme::FG_DEFAULT)),
        ]));
    }

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

fn render_quota_edit_mode(frame: &mut Frame, state: &AppState, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::border_style())
        .title(Span::styled(
            " Edit Quota ",
            Style::default().fg(theme::FG_COMMENT),
        ))
        .style(Style::default().bg(theme::BG_MAIN));

    let five_hour_prefix = if !state.codex_tab.quota_edit_field_weekly {
        "> "
    } else {
        "  "
    };
    let weekly_prefix = if state.codex_tab.quota_edit_field_weekly {
        "> "
    } else {
        "  "
    };

    fn state_style(val: &str) -> ratatui::style::Style {
        match val.to_uppercase().as_str() {
            "HIT" => Style::default().fg(crate::ui::theme::STATUS_ERROR),
            "?" => Style::default().fg(crate::ui::theme::FG_COMMENT),
            _ => Style::default().fg(crate::ui::theme::STATUS_RUNNING),
        }
    }

    let lines = vec![
        Line::from(vec![
            Span::raw(five_hour_prefix),
            Span::styled("5h limit: ", Style::default().fg(theme::FG_COMMENT)),
            Span::styled(
                state.codex_tab.quota_five_hour.clone(),
                state_style(&state.codex_tab.quota_five_hour),
            ),
        ]),
        Line::from(vec![
            Span::raw(weekly_prefix),
            Span::styled("weekly:   ", Style::default().fg(theme::FG_COMMENT)),
            Span::styled(
                state.codex_tab.quota_weekly.clone(),
                state_style(&state.codex_tab.quota_weekly),
            ),
        ]),
        Line::from(Span::raw("")),
        Line::from(vec![
            Span::styled("Space/any", Style::default().fg(theme::FG_COMMENT)),
            Span::raw(" cycle  "),
            Span::styled("Tab", Style::default().fg(theme::FG_COMMENT)),
            Span::raw(" switch field  "),
            Span::styled("Enter", Style::default().fg(theme::FG_COMMENT)),
            Span::raw(" save  "),
            Span::styled("Esc", Style::default().fg(theme::FG_COMMENT)),
            Span::raw(" cancel"),
        ]),
    ];

    frame.render_widget(Paragraph::new(lines).block(block), area);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_bar(pct: u8, bar_width: usize) -> (String, ratatui::style::Color) {
        quota_bar_spans(pct, bar_width)
    }

    #[test]
    fn quota_bar_green_at_50_percent() {
        let (bar, color) = make_bar(50, 20);
        assert_eq!(color, crate::ui::theme::STATUS_RUNNING);
        // 50% of 20 = 10 filled blocks
        assert_eq!(bar.chars().filter(|&c| c == '█').count(), 10);
        assert_eq!(bar.chars().filter(|&c| c == '░').count(), 10);
    }

    #[test]
    fn quota_bar_yellow_at_51_percent() {
        let (_bar, color) = make_bar(51, 20);
        assert_eq!(color, crate::ui::theme::STATUS_WAITING);
    }

    #[test]
    fn quota_bar_red_at_81_percent() {
        let (_bar, color) = make_bar(81, 20);
        assert_eq!(color, crate::ui::theme::STATUS_ERROR);
    }

    #[test]
    fn quota_bar_full_at_100_percent() {
        let (bar, color) = make_bar(100, 20);
        assert_eq!(color, crate::ui::theme::STATUS_ERROR);
        assert_eq!(bar.chars().filter(|&c| c == '█').count(), 20);
        assert_eq!(bar.chars().filter(|&c| c == '░').count(), 0);
    }

    #[test]
    fn quota_bar_empty_at_0_percent() {
        let (bar, color) = make_bar(0, 20);
        assert_eq!(color, crate::ui::theme::STATUS_RUNNING);
        assert_eq!(bar.chars().filter(|&c| c == '█').count(), 0);
        assert_eq!(bar.chars().filter(|&c| c == '░').count(), 20);
    }
}
