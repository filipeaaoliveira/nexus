use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};
use ratatui::Frame;

use crate::claude_profiles::profile::detect_live_claude_profile_id;
use crate::codex_profiles::profile::detect_live_profile_id;
use crate::domain::accounts::{Provider, ProviderFilter};
use crate::services::accounts::storage as storage_service;
use crate::services::runtime::RuntimeSession;

use super::{provider_badge, provider_badge_color};
use crate::ui::theme;

pub fn render(frame: &mut Frame, state: &RuntimeSession, area: Rect) {
    let codex_live = detect_live_profile_id(
        &state.config().codex_profiles_dir,
        &state.config().codex_data_dir.join("auth.json"),
    );
    let claude_live = detect_live_claude_profile_id(
        &state.config().claude_profiles_dir,
        &state.config().claude_data_dir.join(".credentials.json"),
    );

    let accounts = state.root().accounts();
    let filter_label = accounts.provider_filter().label();
    let header_title = format!(" accounts ({}) ", filter_label);

    let block = Block::default()
        .borders(Borders::RIGHT)
        .border_style(theme::border_style())
        .title(Span::styled(
            header_title,
            Style::default().fg(theme::FG_COMMENT),
        ))
        .style(Style::default().bg(theme::BG_MAIN));

    let unified = accounts.visible_entries();

    let items: Vec<ListItem> = unified
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            let entry = *entry;
            let is_selected = accounts.selected_index() == Some(index);
            let is_active = match entry.provider {
                Provider::Codex => {
                    accounts.active_codex_profile_id() == Some(entry.profile_id.as_str())
                }
                Provider::Claude => {
                    accounts.active_claude_profile_id() == Some(entry.profile_id.as_str())
                }
            };

            let profiles_dir = match entry.provider {
                Provider::Codex => &state.config().codex_profiles_dir,
                Provider::Claude => &state.config().claude_profiles_dir,
            };
            let profile_dir = profiles_dir.join(entry.profile_id.as_str());

            let (display_label, is_enabled) = match entry.provider {
                Provider::Codex => storage_service::load_codex_profile(&profile_dir)
                    .map(|profile| (profile.label, profile.enabled))
                    .unwrap_or_else(|_| (entry.profile_id.to_string(), true)),
                Provider::Claude => storage_service::load_claude_profile(&profile_dir)
                    .map(|profile| (profile.label, profile.enabled))
                    .unwrap_or_else(|_| (entry.profile_id.to_string(), true)),
            };

            let quota_indicator = {
                accounts
                    .quota_by_key()
                    .get(&crate::app::state::AccountKey::new(
                        entry.provider,
                        entry.profile_id.clone(),
                    ))
                    .map(|quota| {
                        let windows = quota.effective_windows();
                        let window_bits = windows
                            .iter()
                            .map(|window| {
                                format!(
                                    "{}:{}",
                                    window.label,
                                    super::window_state_symbol(&window.state)
                                )
                            })
                            .collect::<Vec<_>>()
                            .join(" ");
                        let mut indicator = if window_bits.is_empty() {
                            String::new()
                        } else {
                            format!(" [{}]", window_bits)
                        };
                        if let Some(credits) =
                            quota.credits.as_ref().and_then(super::credit_summary)
                        {
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
        let hint = match accounts.provider_filter() {
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
    list_state.select(accounts.selected_index());

    frame.render_stateful_widget(list, area, &mut list_state);
}

#[cfg(test)]
mod tests {
    use crate::domain::accounts::Provider;

    use super::provider_badge;

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
}
