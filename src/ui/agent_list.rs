use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};
use ratatui::Frame;

use crate::app::projection::AgentView;
use crate::services::runtime::RuntimeSession;
use nexus_domain::agents::status::NormalizedStatus;
use crate::ui::shared::format::format_duration;

use super::{theme, RenderData};

pub fn render(frame: &mut Frame, state: &RuntimeSession, render_data: &RenderData<'_>, area: Rect) {
    let block = Block::default()
        .borders(Borders::RIGHT)
        .border_style(theme::border_style())
        .style(Style::default().bg(theme::BG_MAIN));

    if render_data.visible_agents.is_empty() {
        let msg = empty_state_message(state);
        let para = Paragraph::new(vec![
            Line::from(""),
            Line::from(Span::styled(
                format!(" {msg}"),
                Style::default().fg(theme::FG_COMMENT),
            )),
        ])
        .block(block);
        frame.render_widget(para, area);
        return;
    }

    let items: Vec<ListItem> = render_data
        .visible_agents
        .iter()
        .enumerate()
        .map(|(i, view)| {
            render_agent_item(view, state.root().overview().selected_index() == Some(i))
        })
        .collect();

    let list = List::new(items)
        .block(block)
        .highlight_style(theme::selected_style());

    let mut list_state = ListState::default();
    list_state.select(state.root().overview().selected_index());

    frame.render_stateful_widget(list, area, &mut list_state);
}

pub fn empty_state_message(state: &RuntimeSession) -> String {
    if let Some(issue) = state.primary_detector_issue() {
        return issue.to_string();
    }

    let elapsed = state.started_at().elapsed();
    if elapsed >= std::time::Duration::from_secs(state.config().poll_interval_secs) {
        "No agents detected. Start Claude, Codex, or OpenCode to see them here.".to_string()
    } else {
        "Scanning for agents...".to_string()
    }
}

fn render_agent_item(view: &AgentView<'_>, _selected: bool) -> ListItem<'static> {
    let agent = view.agent;
    let elapsed = format_duration(agent.elapsed());
    let subagent_count = agent.subagents().len();

    let line1 = Line::from(vec![
        Span::styled(
            " ▎",
            Style::default().fg(normalized_status_color(agent.normalized().status)),
        ),
        Span::styled(
            format!("{} ", normalized_status_dot(agent.normalized().status)),
            normalized_status_dot_style(agent.normalized().status),
        ),
        Span::styled(
            format!("{} ", view.display_name),
            Style::default().fg(theme::FG_DEFAULT),
        ),
        Span::styled(
            format!("{} ", view.status_label),
            Style::default().fg(normalized_status_color(agent.normalized().status)),
        ),
        Span::styled(
            format!("  {elapsed}"),
            Style::default().fg(theme::FG_DIMMED),
        ),
    ]);

    let project_name = agent
        .cwd()
        .and_then(|cwd| cwd.file_name())
        .map(|name| name.to_string_lossy().into_owned());
    let branch_text = agent.branch().unwrap_or("—");
    let sub_text = if subagent_count > 0 {
        format!("  [{subagent_count} sub]")
    } else {
        String::new()
    };

    let mut line2_spans = vec![Span::styled("    ", Style::default())];
    if let Some(ref project) = project_name {
        line2_spans.push(Span::styled(
            project.clone(),
            Style::default().fg(theme::COLOR_PATH),
        ));
        line2_spans.push(Span::styled("  ", Style::default()));
    }
    line2_spans.push(Span::styled(
        branch_text.to_string(),
        Style::default().fg(theme::COLOR_BRANCH),
    ));
    line2_spans.push(Span::styled(sub_text, Style::default().fg(theme::COLOR_SUBAGENT)));

    let line2 = Line::from(line2_spans);

    ListItem::new(vec![line1, line2])
}

fn normalized_status_color(status: NormalizedStatus) -> ratatui::style::Color {
    match status {
        NormalizedStatus::Working => theme::STATUS_RUNNING,
        NormalizedStatus::NeedsInput => theme::STATUS_WAITING,
        NormalizedStatus::Blocked => theme::STATUS_ERROR,
        NormalizedStatus::Done => theme::STATUS_DONE,
        NormalizedStatus::Unknown => theme::FG_DIMMED,
        _ => theme::FG_DIMMED,
    }
}

fn normalized_status_dot(status: NormalizedStatus) -> &'static str {
    match status {
        NormalizedStatus::Done => "○",
        NormalizedStatus::Working
        | NormalizedStatus::NeedsInput
        | NormalizedStatus::Blocked
        | NormalizedStatus::Unknown => "●",
        _ => "●",
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
