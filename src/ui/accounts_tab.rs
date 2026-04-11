use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph};
use ratatui::Frame;

use crate::app::{build_unified_list, AppState, ConfirmKind, EditField};
use crate::claude_profiles::profile::detect_live_claude_profile_id;
use crate::codex_profiles::profile::{
    detect_live_profile_id, extract_auth_metadata, load_profile_metadata,
};
use crate::codex_profiles::quota::{Confidence, CreditsState, LimitState, QuotaWindow};
use crate::domain::accounts::{Provider, ProviderFilter};
use serde_json::Value;

use super::{footer, header, theme, RenderData};

fn provider_badge(provider: Provider) -> &'static str {
    match provider {
        Provider::Codex => "[CX]",
        Provider::Claude => "[CL]",
    }
}

fn provider_badge_color(provider: Provider) -> ratatui::style::Color {
    match provider {
        Provider::Codex => theme::STATUS_WAITING,
        Provider::Claude => ratatui::style::Color::Cyan,
    }
}

/// Returns (bar_string, fill_color) for a quota percentage (remaining, not used).
/// `bar_width` is the number of block characters to render.
fn quota_bar_spans(pct: u8, bar_width: usize) -> (String, ratatui::style::Color) {
    let filled = ((pct as usize).min(100) * bar_width) / 100;
    let empty = bar_width - filled;
    let bar = format!("{}{}", "█".repeat(filled), "░".repeat(empty));
    // Low remaining = bad (red), medium = warning (yellow), high = good (green)
    let color = if pct <= 50 {
        theme::STATUS_ERROR
    } else if pct <= 80 {
        theme::STATUS_WAITING
    } else {
        theme::STATUS_RUNNING
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

fn window_state_symbol(state: &LimitState) -> &'static str {
    match state {
        LimitState::Hit => "✗",
        LimitState::Available => "✓",
        LimitState::Unknown => "?",
    }
}

fn confidence_label(confidence: Confidence) -> &'static str {
    match confidence {
        Confidence::Exact => "exact",
        Confidence::Parsed => "parsed",
        Confidence::Cached => "cached",
        Confidence::Unavailable => "unavailable",
    }
}

fn credit_summary(credits: &CreditsState) -> Option<String> {
    if !credits.enabled {
        return None;
    }

    let mut parts = Vec::new();
    if credits.unlimited {
        parts.push("unlimited".to_string());
    } else if let Some(balance_label) = credits.balance_label.as_ref() {
        parts.push(balance_label.clone());
    }

    if parts.is_empty() {
        Some("credits".to_string())
    } else {
        Some(format!("credits {}", parts.join(" ")))
    }
}

fn effective_windows(quota: &crate::codex_profiles::quota::QuotaSnapshot) -> Vec<QuotaWindow> {
    quota.effective_windows()
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

    if state.accounts_tab.edit_mode {
        let edit_width = main_chunks[1].width.saturating_sub(8).clamp(36, 64);
        let edit_height = 10u16;
        let edit_area = Rect::new(
            main_chunks[1].x + main_chunks[1].width.saturating_sub(edit_width) / 2,
            main_chunks[1].y + 2,
            edit_width,
            edit_height,
        );
        render_edit_mode(frame, state, edit_area);
    }

    if let Some(ref confirm) = state.accounts_tab.pending_confirm {
        render_confirm_dialog(frame, state, confirm);
    }
}

fn render_profile_list(frame: &mut Frame, state: &AppState, area: Rect) {
    let codex_live = detect_live_profile_id(
        &state.config.codex_profiles_dir,
        &state.config.codex_data_dir.join("auth.json"),
    );
    let claude_live = detect_live_claude_profile_id(
        &state.config.claude_profiles_dir,
        &state.config.claude_data_dir.join(".credentials.json"),
    );

    let filter_label = state.accounts_tab.active_filter.label();
    let header_title = format!(" accounts ({}) ", filter_label);

    let block = Block::default()
        .borders(Borders::RIGHT)
        .border_style(theme::border_style())
        .title(Span::styled(
            header_title,
            Style::default().fg(theme::FG_COMMENT),
        ))
        .style(Style::default().bg(theme::BG_MAIN));

    let unified = build_unified_list(
        &state.accounts_tab.codex_profile_ids,
        &state.accounts_tab.claude_profile_ids,
        state.accounts_tab.active_filter,
    );

    let items: Vec<ListItem> = unified
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            let is_selected = state.accounts_tab.selected_index == Some(index);
            let is_active = match entry.provider {
                Provider::Codex => {
                    state.accounts_tab.codex_active_profile_id.as_deref() == Some(&entry.profile_id)
                }
                Provider::Claude => {
                    state.accounts_tab.claude_active_profile_id.as_deref()
                        == Some(&entry.profile_id)
                }
            };

            let profiles_dir = match entry.provider {
                Provider::Codex => &state.config.codex_profiles_dir,
                Provider::Claude => &state.config.claude_profiles_dir,
            };
            let profile_dir = profiles_dir.join(&entry.profile_id);

            let (display_label, is_enabled) = match entry.provider {
                Provider::Codex => load_profile_metadata(&profile_dir)
                    .map(|m| {
                        (
                            m.label.unwrap_or_else(|| entry.profile_id.clone()),
                            m.enabled,
                        )
                    })
                    .unwrap_or_else(|_| (entry.profile_id.clone(), true)),
                Provider::Claude => {
                    crate::claude_profiles::profile::load_claude_profile_metadata(&profile_dir)
                        .map(|m| {
                            (
                                m.label.unwrap_or_else(|| entry.profile_id.clone()),
                                m.enabled,
                            )
                        })
                        .unwrap_or_else(|_| (entry.profile_id.clone(), true))
                }
            };

            let quota_indicator = {
                let cache = match entry.provider {
                    Provider::Codex => state.accounts_tab.codex_quota_cache.as_ref(),
                    Provider::Claude => state.accounts_tab.claude_quota_cache.as_ref(),
                };
                cache
                    .and_then(|c| c.by_profile.get(&entry.profile_id))
                    .map(|quota| {
                        let windows = effective_windows(quota);
                        let window_bits = windows
                            .iter()
                            .map(|window| {
                                format!("{}:{}", window.label, window_state_symbol(&window.state))
                            })
                            .collect::<Vec<_>>()
                            .join(" ");
                        let mut indicator = if window_bits.is_empty() {
                            String::new()
                        } else {
                            format!(" [{}]", window_bits)
                        };
                        if let Some(credits) = quota.credits.as_ref().and_then(credit_summary) {
                            indicator.push_str(&format!(" {{{credits}}}"));
                        }
                        indicator
                    })
            };

            let live_marker = match entry.provider {
                Provider::Codex => {
                    if codex_live.as_deref() == Some(&entry.profile_id) {
                        " *"
                    } else {
                        ""
                    }
                }
                Provider::Claude => {
                    if claude_live.as_deref() == Some(&entry.profile_id) {
                        " *"
                    } else {
                        ""
                    }
                }
            };

            let prefix = if is_selected { ">" } else { " " };
            let active_suffix = if is_active { " (active)" } else { "" };
            let enabled_suffix = if is_enabled { "" } else { " [disabled]" };
            let quota_str = quota_indicator.unwrap_or_default();
            let badge = provider_badge(entry.provider);

            let line = Line::from(vec![
                Span::styled(
                    format!("{} {} ", prefix, badge),
                    Style::default().fg(provider_badge_color(entry.provider)),
                ),
                Span::styled(
                    format!(
                        "{}{}{}{}{}",
                        display_label, quota_str, live_marker, active_suffix, enabled_suffix
                    ),
                    if is_enabled {
                        Style::default().fg(theme::FG_DEFAULT)
                    } else {
                        Style::default().fg(theme::FG_FADED)
                    },
                ),
            ]);

            ListItem::new(line)
        })
        .collect();

    if unified.is_empty() {
        let hint = match state.accounts_tab.active_filter {
            ProviderFilter::Codex => "No Codex profiles. Press i to import.",
            ProviderFilter::Claude => "No Claude profiles. Press I to import.",
            ProviderFilter::All => "No profiles. Press i (Codex) or I (Claude) to import.",
        };
        let para = Paragraph::new(vec![
            Line::from(""),
            Line::from(Span::styled(
                format!(" {hint}"),
                Style::default().fg(theme::FG_COMMENT),
            )),
        ])
        .block(block);
        frame.render_widget(para, area);
        return;
    }

    let list = List::new(items)
        .block(block)
        .highlight_style(theme::selected_style());

    let mut list_state = ListState::default();
    list_state.select(state.accounts_tab.selected_index);

    frame.render_stateful_widget(list, area, &mut list_state);
}

/// Build one quota row line. Uses a progress bar when `pct` is available,
/// otherwise falls back to a text label. If `suffix` is provided it is
/// appended as a final styled span (used to attach the "as of HH:MM" caption).
pub(crate) fn quota_row(
    label: impl Into<String>,
    pct: Option<u8>,
    limit_state: &crate::codex_profiles::quota::LimitState,
    reset_at: Option<chrono::DateTime<chrono::Utc>>,
    suffix: Option<String>,
) -> Line<'static> {
    let label = label.into();
    let mut spans: Vec<Span<'static>> = if let Some(pct) = pct {
        let (bar, bar_color) = quota_bar_spans(pct, 20);
        let mut s = vec![
            Span::styled(label, Style::default().fg(theme::FG_COMMENT)),
            Span::styled(bar, Style::default().fg(bar_color)),
            Span::styled(format!(" {}%", pct), Style::default().fg(theme::FG_DEFAULT)),
        ];
        if let Some(reset) = reset_at {
            s.push(Span::styled(
                format!("  resets in {}", format_reset_duration(reset)),
                Style::default().fg(theme::FG_COMMENT),
            ));
        }
        s
    } else {
        let text_label = match limit_state {
            crate::codex_profiles::quota::LimitState::Hit => "HIT",
            crate::codex_profiles::quota::LimitState::Available => "ok",
            crate::codex_profiles::quota::LimitState::Unknown => "?",
        };
        let text_color = match limit_state {
            crate::codex_profiles::quota::LimitState::Hit => theme::STATUS_ERROR,
            crate::codex_profiles::quota::LimitState::Available => theme::STATUS_RUNNING,
            crate::codex_profiles::quota::LimitState::Unknown => theme::FG_COMMENT,
        };
        let mut s = vec![
            Span::styled(label, Style::default().fg(theme::FG_COMMENT)),
            Span::styled(text_label, Style::default().fg(text_color)),
        ];
        if let Some(reset) = reset_at {
            let reset_local = chrono::DateTime::<chrono::Local>::from(reset);
            s.push(Span::styled(
                format!("  resets {}", reset_local.format("%a %H:%M")),
                Style::default().fg(theme::FG_COMMENT),
            ));
        }
        s
    };
    if let Some(s) = suffix {
        spans.push(Span::styled(s, Style::default().fg(theme::FG_COMMENT)));
    }
    Line::from(spans)
}

fn render_profile_details(frame: &mut Frame, state: &AppState, area: Rect) {
    let Some((provider, profile_id)) = state.selected_account() else {
        render_detail_placeholder(frame, state, area);
        return;
    };

    let profiles_dir = match provider {
        Provider::Codex => &state.config.codex_profiles_dir,
        Provider::Claude => &state.config.claude_profiles_dir,
    };
    let profile_dir = profiles_dir.join(profile_id);

    // Load metadata and auth info depending on provider
    let (meta_label, meta_email, meta_plan, meta_notes, meta_enabled);
    match provider {
        Provider::Codex => {
            let metadata = load_profile_metadata(&profile_dir).ok();
            let auth_path = profile_dir.join("auth.json");
            let (auth_email, auth_plan) =
                if let Ok(auth_content) = std::fs::read_to_string(&auth_path) {
                    if let Ok(auth_json) = serde_json::from_str::<Value>(&auth_content) {
                        let auth_meta = extract_auth_metadata(&auth_json);
                        (auth_meta.email, auth_meta.plan)
                    } else {
                        (None, None)
                    }
                } else {
                    (None, None)
                };
            meta_label = metadata
                .as_ref()
                .and_then(|m| m.label.clone())
                .unwrap_or_else(|| profile_id.to_string());
            meta_email = metadata
                .as_ref()
                .and_then(|m| m.email.clone())
                .or(auth_email);
            meta_plan = metadata.as_ref().and_then(|m| m.plan.clone()).or(auth_plan);
            meta_notes = metadata.as_ref().and_then(|m| m.notes.clone());
            meta_enabled = metadata.as_ref().map(|m| m.enabled).unwrap_or(true);
        }
        Provider::Claude => {
            let metadata =
                crate::claude_profiles::profile::load_claude_profile_metadata(&profile_dir).ok();
            let auth_path = profile_dir.join(".credentials.json");
            let auth_plan = crate::claude_profiles::auth::ClaudeAuth::load(&auth_path)
                .ok()
                .and_then(|a| a.subscription_type().map(str::to_string));
            meta_label = metadata
                .as_ref()
                .and_then(|m| m.label.clone())
                .unwrap_or_else(|| profile_id.to_string());
            meta_email = metadata.as_ref().and_then(|m| m.email.clone());
            meta_plan = metadata.as_ref().and_then(|m| m.plan.clone()).or(auth_plan);
            meta_notes = metadata.as_ref().and_then(|m| m.notes.clone());
            meta_enabled = metadata.as_ref().map(|m| m.enabled).unwrap_or(true);
        }
    }

    let block = Block::default()
        .borders(Borders::NONE)
        .title(Span::styled(
            " profile details ",
            Style::default().fg(theme::FG_COMMENT),
        ))
        .style(Style::default().bg(theme::BG_MAIN));

    let mut lines = Vec::new();

    // Provider line
    let provider_label = match provider {
        Provider::Codex => "Codex",
        Provider::Claude => "Claude",
    };
    lines.push(Line::from(vec![
        Span::styled("  provider:  ", Style::default().fg(theme::FG_COMMENT)),
        Span::styled(
            provider_label,
            Style::default().fg(provider_badge_color(provider)),
        ),
    ]));

    lines.push(Line::from(vec![
        Span::styled("  label: ", Style::default().fg(theme::FG_COMMENT)),
        Span::styled(&meta_label, Style::default().fg(theme::FG_DEFAULT)),
    ]));

    if let Some(ref email) = meta_email {
        lines.push(Line::from(vec![
            Span::styled("  email: ", Style::default().fg(theme::FG_COMMENT)),
            Span::styled(email, Style::default().fg(theme::FG_DEFAULT)),
        ]));
    }

    if let Some(ref plan) = meta_plan {
        lines.push(Line::from(vec![
            Span::styled("  plan: ", Style::default().fg(theme::FG_COMMENT)),
            Span::styled(plan, Style::default().fg(theme::FG_DEFAULT)),
        ]));
    }

    let enabled = meta_enabled;
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

    let notes = meta_notes;
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

    if let Some(ref quota) = state.accounts_tab.quota {
        lines.push(Line::from(Span::raw("")));
        for window in effective_windows(quota) {
            let used_remaining = window.used_pct.map(|p| 100 - p);
            let prefix = if quota.confidence == Confidence::Exact { "" } else { "~" };
            let label = format!("  {}quota ({}):  ", prefix, window.label);
            lines.push(quota_row(
                label,
                used_remaining,
                &window.state,
                window.reset_at,
                None,
            ));
        }
        if let Some(credits) = quota.credits.as_ref().and_then(credit_summary) {
            lines.push(Line::from(vec![
                Span::styled("  credits: ", Style::default().fg(theme::FG_COMMENT)),
                Span::styled(credits, Style::default().fg(theme::FG_DEFAULT)),
            ]));
        }
        let captured_local = chrono::DateTime::<chrono::Local>::from(quota.captured_at);
        let captured_str = format!(" at {}", captured_local.format("%H:%M"));
        lines.push(Line::from(vec![
            Span::styled("  confidence: ", Style::default().fg(theme::FG_COMMENT)),
            Span::styled(
                confidence_label(quota.confidence),
                Style::default().fg(theme::FG_DEFAULT),
            ),
            Span::styled(captured_str, Style::default().fg(theme::FG_COMMENT)),
        ]));
    }

    if let Some(msg) = state.accounts_tab.status_message.as_ref() {
        lines.push(Line::from(Span::raw("")));
        lines.push(Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled(msg.clone(), Style::default().fg(theme::STATUS_WAITING)),
        ]));
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

    if state.accounts_tab.filter_mode {
        lines.push(Line::from(vec![
            Span::styled("  filter: ", Style::default().fg(theme::FG_COMMENT)),
            Span::styled(
                &state.accounts_tab.filter_input,
                Style::default().fg(theme::FG_DEFAULT),
            ),
            Span::styled("_", Style::default().fg(theme::STATUS_WAITING)),
        ]));
    }

    if let Some(message) = state.accounts_tab.status_message.as_ref() {
        lines.push(Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled(message, Style::default().fg(theme::STATUS_WAITING)),
        ]));
    }

    frame.render_widget(Paragraph::new(lines).block(block), area);
}

fn render_edit_mode(frame: &mut Frame, state: &AppState, area: Rect) {
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
        &state.accounts_tab.edit_label,
        state.accounts_tab.edit_field == EditField::Label,
        Some(theme::STATUS_WAITING),
    ));

    if let Some(ref email) = state.accounts_tab.edit_email {
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled("email: ", Style::default().fg(theme::FG_COMMENT)),
            Span::styled(email, Style::default().fg(theme::FG_DEFAULT)),
        ]));
    }

    if let Some(ref plan) = state.accounts_tab.edit_plan {
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled("plan: ", Style::default().fg(theme::FG_COMMENT)),
            Span::styled(plan, Style::default().fg(theme::FG_DEFAULT)),
        ]));
    }

    lines.push(edit_mode_row(
        "notes",
        &state.accounts_tab.edit_notes,
        state.accounts_tab.edit_field == EditField::Notes,
        Some(theme::STATUS_WAITING),
    ));

    let enabled_status = if state.accounts_tab.edit_enabled {
        "enabled"
    } else {
        "disabled"
    };
    lines.push(edit_mode_row(
        "enabled",
        enabled_status,
        state.accounts_tab.edit_field == EditField::Enabled,
        Some(if state.accounts_tab.edit_enabled {
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

fn render_confirm_dialog(frame: &mut Frame, state: &AppState, confirm: &ConfirmKind) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::AccountsTabState;
    use crate::config::Config;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn render_edit_popup(state: &AppState, area: Rect) -> ratatui::buffer::Buffer {
        let backend = TestBackend::new(area.width, area.height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                render_edit_mode(frame, state, area);
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

    fn make_bar(pct: u8, bar_width: usize) -> (String, ratatui::style::Color) {
        quota_bar_spans(pct, bar_width)
    }

    #[test]
    fn quota_bar_red_at_50_percent_remaining() {
        let (bar, color) = make_bar(50, 20);
        assert_eq!(color, crate::ui::theme::STATUS_ERROR);
        assert_eq!(bar.chars().filter(|&c| c == '█').count(), 10);
        assert_eq!(bar.chars().filter(|&c| c == '░').count(), 10);
    }

    #[test]
    fn quota_bar_yellow_at_51_percent_remaining() {
        let (_bar, color) = make_bar(51, 20);
        assert_eq!(color, crate::ui::theme::STATUS_WAITING);
    }

    #[test]
    fn quota_bar_green_at_81_percent_remaining() {
        let (_bar, color) = make_bar(81, 20);
        assert_eq!(color, crate::ui::theme::STATUS_RUNNING);
    }

    #[test]
    fn quota_bar_full_at_100_percent() {
        let (bar, color) = make_bar(100, 20);
        assert_eq!(color, crate::ui::theme::STATUS_RUNNING);
        assert_eq!(bar.chars().filter(|&c| c == '█').count(), 20);
        assert_eq!(bar.chars().filter(|&c| c == '░').count(), 0);
    }

    #[test]
    fn quota_bar_empty_at_0_percent() {
        let (bar, color) = make_bar(0, 20);
        assert_eq!(color, crate::ui::theme::STATUS_ERROR);
        assert_eq!(bar.chars().filter(|&c| c == '█').count(), 0);
        assert_eq!(bar.chars().filter(|&c| c == '░').count(), 20);
    }

    #[test]
    fn test_provider_badge_codex() {
        let badge = provider_badge(Provider::Codex);
        assert_eq!(badge, "[CX]");
    }

    #[test]
    fn test_provider_badge_claude() {
        let badge = provider_badge(Provider::Claude);
        assert_eq!(badge, "[CL]");
    }

    #[test]
    fn quota_weekly_line_includes_as_of_in_same_line() {
        use crate::codex_profiles::quota::LimitState;
        let line = quota_row(
            "  quota (7d):  ",
            Some(75),
            &LimitState::Available,
            None,
            Some(" at 18:47".to_string()),
        );
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("75%"), "percentage must appear in the line");
        assert!(
            text.contains("at 18:47"),
            "at must appear in the same line as the bar"
        );
    }

    #[test]
    fn quota_five_hour_line_has_no_as_of() {
        use crate::codex_profiles::quota::LimitState;
        let line = quota_row(
            "  quota (5h):  ",
            Some(40),
            &LimitState::Available,
            None,
            None,
        );
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(!text.contains("as of"), "5h line must not contain as of");
    }

    #[test]
    fn edit_popup_shows_cursor_for_active_field() {
        let mut state = AppState::new(Config::default());
        state.accounts_tab = AccountsTabState {
            edit_mode: true,
            edit_label: "Personal".to_string(),
            edit_notes: "Primary".to_string(),
            edit_enabled: true,
            edit_field: EditField::Label,
            ..AccountsTabState::default()
        };

        let buffer = render_edit_popup(&state, Rect::new(0, 0, 48, 10));
        let text = buffer_text(&buffer);

        assert!(
            text.contains("Personal█"),
            "active field should show a cursor-like marker"
        );
    }
}
