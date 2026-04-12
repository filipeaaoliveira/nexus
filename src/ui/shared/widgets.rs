use ratatui::style::Style;
use ratatui::widgets::Block;

use crate::ui::theme;

pub fn background_block() -> Block<'static> {
    Block::default().style(Style::default().bg(theme::BG_MAIN))
}
