use ratatui::style::{Color, Modifier, Style};

use crate::types::PrimaryStatus;

// ── Backgrounds ────────────────────────────────────────────────────

pub const BG_MAIN: Color = Color::Rgb(0x18, 0x16, 0x16);
pub const BG_HEADER: Color = Color::Rgb(0x0d, 0x0c, 0x0c);
pub const BG_SECTION: Color = Color::Rgb(0x1D, 0x1C, 0x19);
pub const BG_SELECTED: Color = Color::Rgb(0x1D, 0x1C, 0x19);
pub const BORDER: Color = Color::Rgb(0x28, 0x27, 0x27);

// ── Foregrounds ────────────────────────────────────────────────────

pub const FG_DEFAULT: Color = Color::Rgb(0xc5, 0xc9, 0xc5);
pub const FG_DIMMED: Color = Color::Rgb(0x9e, 0x9b, 0x93);
pub const FG_COMMENT: Color = Color::Rgb(0x73, 0x7c, 0x73);
pub const FG_FADED: Color = Color::Rgb(0x62, 0x5e, 0x5a);
pub const FG_BORDER: Color = Color::Rgb(0x39, 0x38, 0x36);

// ── Status colors ──────────────────────────────────────────────────

pub const STATUS_WAITING: Color = Color::Rgb(0xc4, 0xb2, 0x8a);
pub const STATUS_RUNNING: Color = Color::Rgb(0x87, 0xa9, 0x87);
pub const STATUS_ERROR: Color = Color::Rgb(0xc4, 0x74, 0x6e);
pub const STATUS_DONE: Color = Color::Rgb(0x62, 0x5e, 0x5a);
pub const STATUS_EXITED: Color = Color::Rgb(0x62, 0x5e, 0x5a);

// ── Semantic colors ────────────────────────────────────────────────

pub const COLOR_PATH: Color = Color::Rgb(0x8b, 0xa4, 0xb0);
pub const COLOR_BRANCH: Color = Color::Rgb(0x89, 0x92, 0xa7);
pub const COLOR_SUBAGENT: Color = Color::Rgb(0x8e, 0xa4, 0xa2);
pub const COLOR_TOOL_EVENT: Color = Color::Rgb(0x8b, 0xa4, 0xb0);
pub const COLOR_SUB_EVENT: Color = Color::Rgb(0x87, 0xa9, 0x87);
pub const COLOR_MSG_EVENT: Color = Color::Rgb(0xc4, 0xb2, 0x8a);

// ── Helper functions ───────────────────────────────────────────────

pub fn status_color(status: PrimaryStatus) -> Color {
    match status {
        PrimaryStatus::Waiting => STATUS_WAITING,
        PrimaryStatus::Running => STATUS_RUNNING,
        PrimaryStatus::Error => STATUS_ERROR,
        PrimaryStatus::Done => STATUS_DONE,
        PrimaryStatus::Exited => STATUS_EXITED,
    }
}

pub fn default_style() -> Style {
    Style::default().fg(FG_DEFAULT).bg(BG_MAIN)
}

pub fn dimmed_style() -> Style {
    Style::default().fg(FG_DIMMED).bg(BG_MAIN)
}

pub fn bar_style() -> Style {
    Style::default().fg(FG_DEFAULT).bg(BG_HEADER)
}

pub fn status_badge_style(status: PrimaryStatus) -> Style {
    Style::default()
        .fg(BG_MAIN)
        .bg(status_color(status))
        .add_modifier(Modifier::BOLD)
}

pub fn status_dot_style(status: PrimaryStatus) -> Style {
    Style::default().fg(status_color(status))
}

pub fn selected_style() -> Style {
    Style::default().fg(FG_DEFAULT).bg(BG_SELECTED)
}

pub fn border_style() -> Style {
    Style::default().fg(FG_BORDER)
}
