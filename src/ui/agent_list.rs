use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState};
use ratatui::Frame;

use crate::app::AppState;
use crate::types::{self, Activity, Agent, NormalizedStatus};

use super::{theme, RenderData};

pub fn render(frame: &mut Frame, state: &AppState, render_data: &RenderData<'_>, area: Rect) {
    let block = Block::default()
        .borders(Borders::RIGHT)
        .border_style(theme::border_style())
        .style(Style::default().bg(theme::BG_MAIN));

    let items: Vec<ListItem> = render_data
        .visible_agents
        .iter()
        .enumerate()
        .map(|(i, agent)| render_agent_item(agent, state.selected == Some(i)))
        .collect();

    let list = List::new(items)
        .block(block)
        .highlight_style(theme::selected_style());

    let mut list_state = ListState::default();
    list_state.select(state.selected);

    frame.render_stateful_widget(list, area, &mut list_state);
}

fn render_agent_item(agent: &Agent, _selected: bool) -> ListItem<'static> {
    let elapsed = types::format_duration(agent.elapsed());
    let subagent_count = agent.subagents.len();
    // When the normalized state has no reason but the activity is ToolCall, show
    // "· tool call" as a hint.  This covers the common race where a poll cycle
    // picks up the tool_use entry and the tool_result in the same batch, so the
    // NormalizedStateChanged event never carries Working(ToolCall).
    let status_text = match &agent.normalized.reason {
        Some(reason) => format!("{} · {}", agent.normalized.status.as_str(), reason.as_str()),
        None if agent.normalized.status == NormalizedStatus::Working
            && agent.activity == Some(Activity::ToolCall) =>
        {
            format!("{} · tool call", agent.normalized.status.as_str())
        }
        // Reachable via two paths:
        //  1. Last entry is CompactBoundary → normalize returns Working(None),
        //     activity returns Compacting.
        //  2. Last entry is the restore user message that follows compact_boundary
        //     → normalize returns Working(None) (User branch), activity returns
        //     Compacting (content check).  normalize_claude_state only returns
        //     Working(Compacting) when CompactBoundary is the *last* entry; the
        //     restore message arrives immediately after and clears the reason.
        None if agent.normalized.status == NormalizedStatus::Working
            && agent.activity == Some(Activity::Compacting) =>
        {
            format!("{} · compacting", agent.normalized.status.as_str())
        }
        None => agent.normalized.status.as_str().to_string(),
    };

    let line1 = Line::from(vec![
        Span::styled(
            " ▎",
            Style::default().fg(normalized_status_color(agent.normalized.status)),
        ),
        Span::styled(
            format!("{} ", normalized_status_dot(agent.normalized.status)),
            normalized_status_dot_style(agent.normalized.status),
        ),
        Span::styled(
            format!("{} ", agent.display_name()),
            Style::default().fg(theme::FG_DEFAULT),
        ),
        Span::styled(
            format!("{status_text} "),
            Style::default().fg(normalized_status_color(agent.normalized.status)),
        ),
        Span::styled(
            format!("  {elapsed}"),
            Style::default().fg(theme::FG_DIMMED),
        ),
    ]);

    let branch_text = agent.branch.as_deref().unwrap_or("—");
    let sub_text = if subagent_count > 0 {
        format!("  [{subagent_count} sub]")
    } else {
        String::new()
    };

    let line2 = Line::from(vec![
        Span::styled("    ", Style::default()),
        Span::styled(
            branch_text.to_string(),
            Style::default().fg(theme::COLOR_BRANCH),
        ),
        Span::styled(sub_text, Style::default().fg(theme::COLOR_SUBAGENT)),
    ]);

    ListItem::new(vec![line1, line2])
}

fn normalized_status_color(status: NormalizedStatus) -> ratatui::style::Color {
    match status {
        NormalizedStatus::Working => theme::STATUS_RUNNING,
        NormalizedStatus::NeedsInput => theme::STATUS_WAITING,
        NormalizedStatus::Blocked => theme::STATUS_ERROR,
        NormalizedStatus::Done => theme::STATUS_DONE,
        NormalizedStatus::Unknown => theme::FG_DIMMED,
    }
}

fn normalized_status_dot(status: NormalizedStatus) -> &'static str {
    match status {
        NormalizedStatus::Done => "○",
        NormalizedStatus::Working
        | NormalizedStatus::NeedsInput
        | NormalizedStatus::Blocked
        | NormalizedStatus::Unknown => "●",
    }
}

fn normalized_status_dot_style(status: NormalizedStatus) -> Style {
    Style::default().fg(normalized_status_color(status))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalized_status_visuals_follow_normalized_state() {
        assert_eq!(normalized_status_dot(NormalizedStatus::NeedsInput), "●");
        assert_eq!(
            normalized_status_color(NormalizedStatus::NeedsInput),
            theme::STATUS_WAITING
        );
        assert_eq!(normalized_status_dot(NormalizedStatus::Done), "○");
        assert_eq!(
            normalized_status_color(NormalizedStatus::Done),
            theme::STATUS_DONE
        );
    }
}
