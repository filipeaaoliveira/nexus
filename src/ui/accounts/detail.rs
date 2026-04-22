use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::app::state::AccountKey;
use crate::app::AccountsMode;
use crate::services::runtime::RuntimeSession;
use nexus_domain::accounts::Provider;
use nexus_domain::accounts::{Confidence, LimitState, QuotaSnapshot};

use super::provider_badge_color;
use crate::ui::theme;

/// Build one quota row line. Uses a progress bar when `pct` is available,
/// otherwise falls back to a text label. If `suffix` is provided it is
/// appended as a final styled span (used to attach the "as of HH:MM" caption).
pub(crate) fn quota_row(
    label: impl Into<String>,
    pct: Option<u8>,
    limit_state: &LimitState,
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
            LimitState::Hit => "HIT",
            LimitState::Available => "ok",
            LimitState::Unknown => "?",
            _ => "?",
        };
        let text_color = match limit_state {
            LimitState::Hit => theme::STATUS_ERROR,
            LimitState::Available => theme::STATUS_RUNNING,
            LimitState::Unknown => theme::FG_COMMENT,
            _ => theme::FG_COMMENT,
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

pub fn render(frame: &mut Frame, state: &RuntimeSession, area: Rect) {
    let Some((provider, profile_id)) = state.selected_account() else {
        render_placeholder(frame, state, area);
        return;
    };

    let key = AccountKey::new(provider, profile_id.to_string());
    let profile_details = state.root().accounts().profile_by_key(&key);
    let block = Block::default()
        .borders(Borders::NONE)
        .title(Span::styled(
            " profile details ",
            Style::default().fg(theme::FG_COMMENT),
        ))
        .style(Style::default().bg(theme::BG_MAIN));

    let mut lines = Vec::new();

    let provider_label = match provider {
        Provider::Codex => "Codex",
        Provider::Claude => "Claude",
        _ => "Unknown",
    };
    lines.push(Line::from(vec![
        Span::styled("  provider:  ", Style::default().fg(theme::FG_COMMENT)),
        Span::styled(
            provider_label,
            Style::default().fg(provider_badge_color(provider)),
        ),
    ]));

    if let Some(p) = profile_details {
        lines.push(Line::from(vec![
            Span::styled("  label: ", Style::default().fg(theme::FG_COMMENT)),
            Span::styled(p.label.clone(), Style::default().fg(theme::FG_DEFAULT)),
        ]));

        if let Some(ref email) = p.email {
            lines.push(Line::from(vec![
                Span::styled("  email: ", Style::default().fg(theme::FG_COMMENT)),
                Span::styled(email.clone(), Style::default().fg(theme::FG_DEFAULT)),
            ]));
        }

        if let Some(ref plan) = p.plan {
            lines.push(Line::from(vec![
                Span::styled("  plan: ", Style::default().fg(theme::FG_COMMENT)),
                Span::styled(plan.clone(), Style::default().fg(theme::FG_DEFAULT)),
            ]));
        }

        lines.push(Line::from(vec![
            Span::styled("  enabled: ", Style::default().fg(theme::FG_COMMENT)),
            Span::styled(
                if p.enabled { "yes" } else { "no" },
                if p.enabled {
                    Style::default().fg(theme::STATUS_RUNNING)
                } else {
                    Style::default().fg(theme::STATUS_EXITED)
                },
            ),
        ]));

        if let Some(ref notes) = p.notes {
            if !notes.is_empty() {
                lines.push(Line::from(Span::raw("")));
                lines.push(Line::from(vec![Span::styled(
                    "  notes: ",
                    Style::default().fg(theme::FG_COMMENT),
                )]));
                for note_line in notes.lines() {
                    lines.push(Line::from(vec![
                        Span::styled("    ", Style::default()),
                        Span::styled(
                            note_line.to_string(),
                            Style::default().fg(theme::FG_DEFAULT),
                        ),
                    ]));
                }
            }
        }
    } else {
        lines.push(Line::from(vec![
            Span::styled("  label: ", Style::default().fg(theme::FG_COMMENT)),
            Span::styled(
                profile_id.to_string(),
                Style::default().fg(theme::FG_DEFAULT),
            ),
        ]));
    }

    if let Some(quota) = state.root().accounts().quota_by_key().get(&key) {
        append_quota_lines(&mut lines, quota);
    }

    if let Some(msg) = state.root().accounts().status_message() {
        lines.push(Line::from(Span::raw("")));
        lines.push(Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled(msg.to_string(), Style::default().fg(theme::STATUS_WAITING)),
        ]));
    }

    frame.render_widget(Paragraph::new(lines).block(block), area);
}

fn append_quota_lines(lines: &mut Vec<Line<'static>>, quota: &QuotaSnapshot) {
    lines.push(Line::from(Span::raw("")));
    for window in quota.windows() {
        let used_remaining = window.used_pct.map(|p| 100 - p);
        let prefix = if quota.confidence == Confidence::Exact {
            ""
        } else {
            "~"
        };
        let label = format!("  {}quota ({}):  ", prefix, window.kind.label());
        lines.push(quota_row(
            label,
            used_remaining,
            &window.state,
            window.reset_at,
            None,
        ));
    }
    if let Some(credits) = quota.credits.as_ref().and_then(super::credit_summary) {
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
            super::confidence_label(quota.confidence),
            Style::default().fg(theme::FG_DEFAULT),
        ),
        Span::styled(captured_str, Style::default().fg(theme::FG_COMMENT)),
    ]));
}

fn render_placeholder(frame: &mut Frame, state: &RuntimeSession, area: Rect) {
    let block = Block::default()
        .borders(Borders::NONE)
        .title(Span::styled(
            " profile details ",
            Style::default().fg(theme::FG_COMMENT),
        ))
        .style(Style::default().bg(theme::BG_MAIN));

    let mut lines = Vec::new();

    if let AccountsMode::Filtering { input } = state.root().accounts().mode() {
        lines.push(Line::from(vec![
            Span::styled("  filter: ", Style::default().fg(theme::FG_COMMENT)),
            Span::styled(input.clone(), Style::default().fg(theme::FG_DEFAULT)),
            Span::styled("_", Style::default().fg(theme::STATUS_WAITING)),
        ]));
    }

    if let Some(message) = state.root().accounts().status_message() {
        lines.push(Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled(
                message.to_string(),
                Style::default().fg(theme::STATUS_WAITING),
            ),
        ]));
    }

    frame.render_widget(Paragraph::new(lines).block(block), area);
}

fn quota_bar_spans(pct: u8, bar_width: usize) -> (String, ratatui::style::Color) {
    let filled = ((pct as usize).min(100) * bar_width) / 100;
    let empty = bar_width - filled;
    let bar = format!("{}{}", "█".repeat(filled), "░".repeat(empty));
    let color = if pct <= 50 {
        theme::STATUS_ERROR
    } else if pct <= 80 {
        theme::STATUS_WAITING
    } else {
        theme::STATUS_RUNNING
    };
    (bar, color)
}

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

#[cfg(test)]
mod tests {
    use nexus_domain::accounts::LimitState;

    use super::{quota_bar_spans, quota_row};

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
    fn quota_weekly_line_includes_as_of_in_same_line() {
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
}
