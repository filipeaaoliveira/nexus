use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Quit,
    MoveUp,
    MoveDown,
    NextTab,
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
    AccountsConfirm,
    RenameChar(char),
    RenameBackspace,
    RenameConfirm,
    Tick,
    AccountsEditStart,
    AccountsEditChar(char),
    AccountsEditBackspace,
    AccountsEditToggleEnabled,
    AccountsEditNextField,
    AccountsEditPrevField,
    AccountsEditConfirm,
    AccountsEditCancel,
    AccountsFilterCycle,
    AccountsImportAuth,
    AccountsImportClaudeAuth,
    AccountsWriteToHome,
    AccountsRefreshQuota,
}

pub fn poll_action(
    timeout: Duration,
    filter_mode: bool,
    rename_mode: bool,
    edit_mode: bool,
    confirm_mode: bool,
) -> anyhow::Result<Option<Action>> {
    if event::poll(timeout)? {
        match event::read()? {
            Event::Key(key) => {
                return Ok(action_for_key(
                    key,
                    filter_mode,
                    rename_mode,
                    edit_mode,
                    confirm_mode,
                ));
            }
            // Resize events are consumed here; the next terminal.draw() call will
            // automatically use the updated dimensions.
            Event::Resize(_, _) => {}
            _ => {}
        }
    }
    Ok(None)
}

fn action_for_key(
    key: crossterm::event::KeyEvent,
    filter_mode: bool,
    rename_mode: bool,
    edit_mode: bool,
    confirm_mode: bool,
) -> Option<Action> {
    if key.kind != KeyEventKind::Press {
        return None;
    }

    if edit_mode {
        return match key.code {
            KeyCode::Esc => Some(Action::AccountsEditCancel),
            KeyCode::Enter => Some(Action::AccountsEditConfirm),
            KeyCode::Char(' ') => Some(Action::AccountsEditToggleEnabled),
            KeyCode::Tab => Some(Action::AccountsEditNextField),
            KeyCode::BackTab => Some(Action::AccountsEditPrevField),
            KeyCode::Char(c) => Some(Action::AccountsEditChar(c)),
            KeyCode::Backspace => Some(Action::AccountsEditBackspace),
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

    if confirm_mode {
        return match key.code {
            KeyCode::Enter => Some(Action::AccountsConfirm),
            KeyCode::Esc => Some(Action::DismissOverlay),
            _ => None,
        };
    }

    match key.code {
        KeyCode::Char('q') => Some(Action::Quit),
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => Some(Action::Quit),
        KeyCode::Char('j') | KeyCode::Down => Some(Action::MoveDown),
        KeyCode::Char('k') | KeyCode::Up => Some(Action::MoveUp),
        KeyCode::Tab => Some(Action::NextTab),
        KeyCode::Char('s') => Some(Action::CycleSortMode),
        KeyCode::Char('/') => Some(Action::StartFilter),
        KeyCode::Char('?') => Some(Action::ShowHelp),
        KeyCode::Char('d') => Some(Action::RemoveAgent),
        KeyCode::Char('D') => Some(Action::RemoveExited),
        KeyCode::Char('r') => Some(Action::StartRename),
        KeyCode::Char('i') => Some(Action::AccountsImportAuth),
        KeyCode::Char('I') => Some(Action::AccountsImportClaudeAuth),
        KeyCode::Char('w') => Some(Action::AccountsWriteToHome),
        KeyCode::Char('e') => Some(Action::AccountsEditStart),
        KeyCode::Char('R') => Some(Action::AccountsRefreshQuota),
        KeyCode::Char('f') => Some(Action::AccountsFilterCycle),
        KeyCode::Enter => Some(Action::AccountsConfirm),
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
                false,
                false,
                false
            ),
            Some(Action::AccountsImportAuth)
        );
        assert_eq!(
            action_for_key(
                KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
                false,
                false,
                false,
                false
            ),
            Some(Action::AccountsConfirm)
        );
    }

    #[test]
    fn test_uppercase_i_maps_to_import_claude_auth() {
        assert_eq!(
            action_for_key(
                KeyEvent::new(KeyCode::Char('I'), KeyModifiers::NONE),
                false,
                false,
                false,
                false
            ),
            Some(Action::AccountsImportClaudeAuth)
        );
    }

    #[test]
    fn test_uppercase_i_absorbed_by_modal_modes() {
        let key = KeyEvent::new(KeyCode::Char('I'), KeyModifiers::NONE);
        // edit_mode: I becomes a char-input event, not an import action
        assert_eq!(
            action_for_key(key, false, false, true, false),
            Some(Action::AccountsEditChar('I'))
        );
        // filter_mode: same — I feeds into the filter input
        assert_eq!(
            action_for_key(key, true, false, false, false),
            Some(Action::FilterChar('I'))
        );
        // confirm_mode: I is absorbed (only Enter/Esc are active)
        assert_eq!(action_for_key(key, false, false, false, true), None);
    }

    #[test]
    fn test_enter_stays_filter_confirm_while_filtering() {
        assert_eq!(
            action_for_key(
                KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
                true,
                false,
                false,
                false
            ),
            Some(Action::FilterConfirm)
        );
    }

    #[test]
    fn test_edit_mode_keys_map_correctly() {
        assert_eq!(
            action_for_key(
                KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE),
                false,
                false,
                false,
                false
            ),
            Some(Action::AccountsEditStart)
        );

        assert_eq!(
            action_for_key(
                KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
                false,
                false,
                true,
                false
            ),
            Some(Action::AccountsEditConfirm)
        );

        assert_eq!(
            action_for_key(
                KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE),
                false,
                false,
                true,
                false
            ),
            Some(Action::AccountsEditToggleEnabled)
        );
    }
}
