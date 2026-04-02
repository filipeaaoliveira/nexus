use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Quit,
    MoveUp,
    MoveDown,
    ToggleGroupByWorktree,
    CycleSortMode,
    StartFilter,
    FilterChar(char),
    FilterBackspace,
    FilterConfirm,
    ShowHelp,
    DismissOverlay,
    RemoveAgent,
    RemoveExited,
    StartRename,
    RenameChar(char),
    RenameBackspace,
    RenameConfirm,
    Tick,
}

pub fn poll_action(
    timeout: Duration,
    filter_mode: bool,
    rename_mode: bool,
) -> anyhow::Result<Option<Action>> {
    if event::poll(timeout)? {
        if let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press {
                return Ok(None);
            }

            if rename_mode {
                let action = match key.code {
                    KeyCode::Esc => Some(Action::DismissOverlay),
                    KeyCode::Enter => Some(Action::RenameConfirm),
                    KeyCode::Backspace => Some(Action::RenameBackspace),
                    KeyCode::Char(c) => Some(Action::RenameChar(c)),
                    _ => None,
                };
                return Ok(action);
            }

            if filter_mode {
                let action = match key.code {
                    KeyCode::Esc => Some(Action::DismissOverlay),
                    KeyCode::Enter => Some(Action::FilterConfirm),
                    KeyCode::Backspace => Some(Action::FilterBackspace),
                    KeyCode::Char(c) => Some(Action::FilterChar(c)),
                    _ => None,
                };
                return Ok(action);
            }

            let action = match key.code {
                KeyCode::Char('q') => Some(Action::Quit),
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    Some(Action::Quit)
                }
                KeyCode::Char('j') | KeyCode::Down => Some(Action::MoveDown),
                KeyCode::Char('k') | KeyCode::Up => Some(Action::MoveUp),
                KeyCode::Char('g') => Some(Action::ToggleGroupByWorktree),
                KeyCode::Char('s') => Some(Action::CycleSortMode),
                KeyCode::Char('/') => Some(Action::StartFilter),
                KeyCode::Char('?') => Some(Action::ShowHelp),
                KeyCode::Char('d') => Some(Action::RemoveAgent),
                KeyCode::Char('D') => Some(Action::RemoveExited),
                KeyCode::Char('r') => Some(Action::StartRename),
                KeyCode::Esc => Some(Action::DismissOverlay),
                _ => None,
            };
            return Ok(action);
        }
    }
    Ok(None)
}
