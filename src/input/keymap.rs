use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::app::{
    AccountsAction, AccountsConfirmAction, Action as SemanticAction, OverviewAction, SystemAction,
    WorkspaceAction,
};

use super::{
    accounts_filter_action, append_char, edit_buffer, next_edit_field, overview_filter_action,
    pop_char, set_edit_field, Action, Context, Mode, RuntimeAction,
};

pub fn action_for_key(key: KeyEvent, context: &Context) -> Option<Action> {
    if key.kind != KeyEventKind::Press {
        return None;
    }

    match &context.mode {
        Mode::Help => map_help_key(key),
        Mode::OverviewFilter { input } => map_overview_filter_key(key, input),
        Mode::OverviewRename { input } => map_overview_rename_key(key, input),
        Mode::AccountsFilter { input } => map_accounts_filter_key(key, input),
        Mode::AccountsEdit { draft, field } => map_accounts_edit_key(key, draft, *field),
        Mode::AccountsConfirm => map_accounts_confirm_key(key),
        Mode::Normal => map_normal_key(key, context),
    }
}

fn map_help_key(key: KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Char('q') => Some(Action::App(Box::new(SemanticAction::QuitRequested))),
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            Some(Action::App(Box::new(SemanticAction::QuitRequested)))
        }
        KeyCode::Char('j') | KeyCode::Down => Some(Action::App(Box::new(SemanticAction::System(
            SystemAction::ScrollHelpDown,
        )))),
        KeyCode::Char('k') | KeyCode::Up => Some(Action::App(Box::new(SemanticAction::System(
            SystemAction::ScrollHelpUp,
        )))),
        KeyCode::Char('?') => Some(Action::Runtime(RuntimeAction::ToggleHelp)),
        KeyCode::Esc => Some(Action::Runtime(RuntimeAction::DismissCurrentMode)),
        _ => None,
    }
}

fn map_overview_filter_key(key: KeyEvent, input: &str) -> Option<Action> {
    match key.code {
        KeyCode::Esc => Some(Action::App(Box::new(SemanticAction::Overview(
            OverviewAction::CancelMode,
        )))),
        KeyCode::Enter => Some(Action::App(Box::new(SemanticAction::Overview(
            OverviewAction::ConfirmFilter,
        )))),
        KeyCode::Backspace => Some(Action::App(Box::new(overview_filter_action(pop_char(
            input,
        ))))),
        KeyCode::Char(c) => Some(Action::App(Box::new(overview_filter_action(append_char(
            input, c,
        ))))),
        _ => None,
    }
}

fn map_overview_rename_key(key: KeyEvent, input: &str) -> Option<Action> {
    match key.code {
        KeyCode::Esc => Some(Action::App(Box::new(SemanticAction::Overview(
            OverviewAction::CancelMode,
        )))),
        KeyCode::Enter => Some(Action::App(Box::new(SemanticAction::Overview(
            OverviewAction::ConfirmRename,
        )))),
        KeyCode::Backspace => Some(Action::App(Box::new(SemanticAction::Overview(
            OverviewAction::UpdateRenameInput(pop_char(input)),
        )))),
        KeyCode::Char(c) => Some(Action::App(Box::new(SemanticAction::Overview(
            OverviewAction::UpdateRenameInput(append_char(input, c)),
        )))),
        _ => None,
    }
}

fn map_accounts_filter_key(key: KeyEvent, input: &str) -> Option<Action> {
    match key.code {
        KeyCode::Esc => Some(Action::App(Box::new(SemanticAction::Accounts(
            AccountsAction::CancelMode,
        )))),
        KeyCode::Enter => Some(Action::App(Box::new(SemanticAction::Accounts(
            AccountsAction::Confirm,
        )))),
        KeyCode::Backspace => Some(Action::App(Box::new(accounts_filter_action(pop_char(
            input,
        ))))),
        KeyCode::Char(c) => Some(Action::App(Box::new(accounts_filter_action(append_char(
            input, c,
        ))))),
        _ => None,
    }
}

fn map_accounts_edit_key(
    key: KeyEvent,
    draft: &crate::app::EditableProfile,
    field: crate::app::AccountsEditField,
) -> Option<Action> {
    match key.code {
        KeyCode::Esc => Some(Action::App(Box::new(SemanticAction::Accounts(
            AccountsAction::CancelMode,
        )))),
        KeyCode::Enter => Some(Action::App(Box::new(SemanticAction::Accounts(
            AccountsAction::Confirm,
        )))),
        KeyCode::Char(' ') => Some(Action::App(Box::new(SemanticAction::Accounts(
            AccountsAction::ToggleEnabled,
        )))),
        KeyCode::Tab => Some(Action::App(Box::new(set_edit_field(
            draft,
            next_edit_field(field, false),
        )))),
        KeyCode::BackTab => Some(Action::App(Box::new(set_edit_field(
            draft,
            next_edit_field(field, true),
        )))),
        KeyCode::Backspace => Some(Action::App(Box::new(SemanticAction::Accounts(
            AccountsAction::UpdateEditBuffer(pop_char(edit_buffer(draft, field))),
        )))),
        KeyCode::Char(c) => Some(Action::App(Box::new(SemanticAction::Accounts(
            AccountsAction::UpdateEditBuffer(append_char(edit_buffer(draft, field), c)),
        )))),
        _ => None,
    }
}

fn map_accounts_confirm_key(key: KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Enter => Some(Action::App(Box::new(SemanticAction::Accounts(
            AccountsAction::Confirm,
        )))),
        KeyCode::Esc => Some(Action::App(Box::new(SemanticAction::Accounts(
            AccountsAction::CancelMode,
        )))),
        _ => None,
    }
}

fn map_normal_key(key: KeyEvent, context: &Context) -> Option<Action> {
    match key.code {
        KeyCode::Char('q') => Some(Action::App(Box::new(SemanticAction::QuitRequested))),
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            Some(Action::App(Box::new(SemanticAction::QuitRequested)))
        }
        KeyCode::Char('j') | KeyCode::Down => {
            Some(Action::App(Box::new(match context.workspace {
                crate::app::Workspace::Overview => {
                    SemanticAction::Overview(OverviewAction::SelectNext)
                }
                crate::app::Workspace::Accounts => {
                    SemanticAction::Accounts(AccountsAction::SelectNext)
                }
            })))
        }
        KeyCode::Char('k') | KeyCode::Up => Some(Action::App(Box::new(match context.workspace {
            crate::app::Workspace::Overview => {
                SemanticAction::Overview(OverviewAction::SelectPrevious)
            }
            crate::app::Workspace::Accounts => {
                SemanticAction::Accounts(AccountsAction::SelectPrevious)
            }
        }))),
        KeyCode::Tab => Some(Action::App(Box::new(SemanticAction::Workspace(
            WorkspaceAction::Next,
        )))),
        KeyCode::Char('s') if context.workspace == crate::app::Workspace::Overview => {
            Some(Action::App(Box::new(SemanticAction::Overview(
                OverviewAction::CycleSortMode,
            ))))
        }
        KeyCode::Char('/') => Some(Action::App(Box::new(match context.workspace {
            crate::app::Workspace::Overview => {
                SemanticAction::Overview(OverviewAction::StartFiltering)
            }
            crate::app::Workspace::Accounts => {
                SemanticAction::Accounts(AccountsAction::StartFiltering)
            }
        }))),
        KeyCode::Char('?') => Some(Action::Runtime(RuntimeAction::ToggleHelp)),
        KeyCode::Char('d') => Some(match context.workspace {
            crate::app::Workspace::Overview => Action::Runtime(RuntimeAction::DismissSelectedAgent),
            crate::app::Workspace::Accounts => Action::App(Box::new(SemanticAction::Accounts(
                AccountsAction::RequestConfirmation(AccountsConfirmAction::DeleteProfile),
            ))),
        }),
        KeyCode::Char('D') if context.workspace == crate::app::Workspace::Overview => {
            Some(Action::Runtime(RuntimeAction::DismissExitedAgents))
        }
        KeyCode::Char('r') if context.workspace == crate::app::Workspace::Overview => {
            Some(Action::Runtime(RuntimeAction::StartOverviewRename))
        }
        KeyCode::Char('i') if context.workspace == crate::app::Workspace::Accounts => {
            Some(Action::Runtime(RuntimeAction::ImportCodexProfile))
        }
        KeyCode::Char('I') if context.workspace == crate::app::Workspace::Accounts => {
            Some(Action::Runtime(RuntimeAction::ImportClaudeProfile))
        }
        KeyCode::Char('w') if context.workspace == crate::app::Workspace::Accounts => {
            Some(Action::Runtime(RuntimeAction::RequestWriteProfile))
        }
        KeyCode::Char('e') if context.workspace == crate::app::Workspace::Accounts => {
            Some(Action::Runtime(RuntimeAction::StartAccountsEdit))
        }
        KeyCode::Char('R') if context.workspace == crate::app::Workspace::Accounts => {
            Some(Action::Runtime(RuntimeAction::RefreshSelectedQuota))
        }
        KeyCode::Char('f') if context.workspace == crate::app::Workspace::Accounts => {
            Some(Action::App(Box::new(SemanticAction::Accounts(
                AccountsAction::SetProviderFilter(context.provider_filter.next()),
            ))))
        }
        KeyCode::Esc => Some(Action::Runtime(RuntimeAction::DismissCurrentMode)),
        _ => None,
    }
}
