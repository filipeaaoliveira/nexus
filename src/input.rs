use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Quit,
    MoveUp,
    MoveDown,
    NextTab,
    PreviousTab,
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
    CodexLaunchSelected,
    CodexImportAuth,
    RenameChar(char),
    RenameBackspace,
    RenameConfirm,
    Tick,
    CodexSessionPickerUp,
    CodexSessionPickerDown,
    CodexSessionPickerConfirm,
    CodexSessionPickerCancel,
}

pub fn poll_action(
    timeout: Duration,
    filter_mode: bool,
    rename_mode: bool,
    session_picker_mode: bool,
) -> anyhow::Result<Option<Action>> {
    if event::poll(timeout)? {
        if let Event::Key(key) = event::read()? {
            return Ok(action_for_key(
                key,
                filter_mode,
                rename_mode,
                session_picker_mode,
            ));
        }
    }
    Ok(None)
}

fn action_for_key(
    key: crossterm::event::KeyEvent,
    filter_mode: bool,
    rename_mode: bool,
    session_picker_mode: bool,
) -> Option<Action> {
    if key.kind != KeyEventKind::Press {
        return None;
    }

    if session_picker_mode {
        return match key.code {
            KeyCode::Esc => Some(Action::CodexSessionPickerCancel),
            KeyCode::Enter => Some(Action::CodexSessionPickerConfirm),
            KeyCode::Char('j') | KeyCode::Down => Some(Action::CodexSessionPickerDown),
            KeyCode::Char('k') | KeyCode::Up => Some(Action::CodexSessionPickerUp),
            _ => None,
        };
    }

    if rename_mode {
        return match key.code {
            KeyCode::Esc => Some(Action::DismissOverlay),
            KeyCode::Enter => Some(Action::RenameConfirm),
            KeyCode::Backspace => Some(Action::RenameBackspace),
            KeyCode::Char(c) => Some(Action::RenameChar(c)),
            _ => None,
        };
    }

    if filter_mode {
        return match key.code {
            KeyCode::Esc => Some(Action::DismissOverlay),
            KeyCode::Enter => Some(Action::FilterConfirm),
            KeyCode::Backspace => Some(Action::FilterBackspace),
            KeyCode::Char(c) => Some(Action::FilterChar(c)),
            _ => None,
        };
    }

    match key.code {
        KeyCode::Char('q') => Some(Action::Quit),
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => Some(Action::Quit),
        KeyCode::Char('j') | KeyCode::Down => Some(Action::MoveDown),
        KeyCode::Char('k') | KeyCode::Up => Some(Action::MoveUp),
        KeyCode::Tab => Some(Action::NextTab),
        KeyCode::BackTab => Some(Action::PreviousTab),
        KeyCode::Char('s') => Some(Action::CycleSortMode),
        KeyCode::Char('/') => Some(Action::StartFilter),
        KeyCode::Char('?') => Some(Action::ShowHelp),
        KeyCode::Char('d') => Some(Action::RemoveAgent),
        KeyCode::Char('D') => Some(Action::RemoveExited),
        KeyCode::Char('r') => Some(Action::StartRename),
        KeyCode::Char('i') => Some(Action::CodexImportAuth),
        KeyCode::Enter => Some(Action::CodexLaunchSelected),
        KeyCode::Esc => Some(Action::DismissOverlay),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{action_for_key, Action};
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    #[test]
    fn test_codex_shortcuts_map_when_not_in_input_modes() {
        assert_eq!(
            action_for_key(
                KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE),
                false,
                false
            ),
            Some(Action::CodexImportAuth)
        );
        assert_eq!(
            action_for_key(
                KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
                false,
                false
            ),
            Some(Action::CodexLaunchSelected)
        );
    }

    #[test]
    fn test_enter_stays_filter_confirm_while_filtering() {
        assert_eq!(
            action_for_key(
                KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
                true,
                false
            ),
            Some(Action::FilterConfirm)
        );
    }
}
