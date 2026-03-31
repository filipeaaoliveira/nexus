use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState};
use ratatui::Frame;

use crate::app::AppState;
use crate::types::{self, Agent, PrimaryStatus};

use super::theme;

pub fn render(frame: &mut Frame, state: &AppState, area: Rect) {
    let visible = state.visible_agents();

    let block = Block::default()
        .borders(Borders::RIGHT)
        .border_style(theme::border_style())
        .style(Style::default().bg(theme::BG_MAIN));

    let items: Vec<ListItem> = visible
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
    let status_dot = match agent.status {
        PrimaryStatus::Done | PrimaryStatus::Exited => "○",
        _ => "●",
    };

    let elapsed = types::format_duration(agent.elapsed());
    let subagent_count = agent.subagents.len();

    let line1 = Line::from(vec![
        Span::styled(" ▎", Style::default().fg(theme::status_color(agent.status))),
        Span::styled(
            format!("{status_dot} "),
            theme::status_dot_style(agent.status),
        ),
        Span::styled(
            format!("{} ", agent.tool.as_str()),
            Style::default().fg(theme::FG_DEFAULT),
        ),
        Span::styled(
            agent.status.as_str().to_string(),
            Style::default().fg(theme::status_color(agent.status)),
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
