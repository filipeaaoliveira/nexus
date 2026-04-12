pub mod confirm;
pub mod detail;
pub mod edit;
pub mod footer;
pub mod list;

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::Style;
use ratatui::widgets::Block;
use ratatui::Frame;

use crate::app::{AccountsConfirmAction, AccountsMode, AppState, ConfirmKind};
use crate::codex_profiles::quota::{Confidence, CreditsState, LimitState};
use crate::domain::accounts::Provider;
use crate::ui::{header, theme, RenderData};

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

    list::render(frame, state, main_chunks[0]);
    detail::render(frame, state, main_chunks[1]);
    footer::render(frame, state, outer[2]);

    if matches!(state.root.accounts.mode, AccountsMode::Editing { .. }) {
        let edit_width = main_chunks[1].width.saturating_sub(8).clamp(36, 64);
        let edit_height = 10u16;
        let edit_area = Rect::new(
            main_chunks[1].x + main_chunks[1].width.saturating_sub(edit_width) / 2,
            main_chunks[1].y + 2,
            edit_width,
            edit_height,
        );
        edit::render(frame, state, edit_area);
    }

    if let AccountsMode::Confirming { action } = state.root.accounts.mode {
        let confirm = match action {
            AccountsConfirmAction::DeleteProfile => ConfirmKind::DeleteProfile,
            AccountsConfirmAction::WriteProfileToHome => ConfirmKind::WriteToHome,
        };
        confirm::render(frame, state, &confirm);
    }
}

pub(crate) fn provider_badge(provider: Provider) -> &'static str {
    match provider {
        Provider::Codex => "[CX]",
        Provider::Claude => "[CL]",
    }
}

pub(crate) fn provider_badge_color(provider: Provider) -> ratatui::style::Color {
    match provider {
        Provider::Codex => theme::STATUS_WAITING,
        Provider::Claude => ratatui::style::Color::Cyan,
    }
}

pub(crate) fn window_state_symbol(state: &LimitState) -> &'static str {
    match state {
        LimitState::Hit => "✗",
        LimitState::Available => "✓",
        LimitState::Unknown => "?",
    }
}

pub(crate) fn confidence_label(confidence: Confidence) -> &'static str {
    match confidence {
        Confidence::Exact => "exact",
        Confidence::Parsed => "parsed",
        Confidence::Cached => "cached",
        Confidence::Unavailable => "unavailable",
    }
}

pub(crate) fn credit_summary(credits: &CreditsState) -> Option<String> {
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
