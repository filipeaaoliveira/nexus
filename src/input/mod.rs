use std::time::Duration;

use crossterm::event::{self, Event};

use crate::app::{
    AccountsEditField, AccountsMode, Action as SemanticAction, ProfileDraft, Workspace,
};
use crate::domain::accounts::ProviderFilter;

mod keymap;
pub use keymap::action_for_key;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    App(Box<SemanticAction>),
    Runtime(RuntimeAction),
    Tick,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeAction {
    ToggleHelp,
    DismissCurrentMode,
    DismissSelectedAgent,
    DismissExitedAgents,
    StartOverviewRename,
    ImportCodexProfile,
    ImportClaudeProfile,
    RequestWriteProfile,
    StartAccountsEdit,
    RefreshSelectedQuota,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Context {
    pub workspace: Workspace,
    pub mode: Mode,
    pub provider_filter: ProviderFilter,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Help,
    OverviewFilter {
        input: String,
    },
    OverviewRename {
        input: String,
    },
    AccountsFilter {
        input: String,
    },
    AccountsEdit {
        draft: ProfileDraft,
        field: AccountsEditField,
    },
    AccountsConfirm,
}

pub fn poll_action(timeout: Duration, context: &Context) -> anyhow::Result<Option<Action>> {
    if event::poll(timeout)? {
        match event::read()? {
            Event::Key(key) => return Ok(keymap::action_for_key(key, context)),
            Event::Resize(_, _) => {}
            _ => {}
        }
    }
    Ok(None)
}

impl Context {
    pub fn overview_normal() -> Self {
        Self {
            workspace: Workspace::Overview,
            mode: Mode::Normal,
            provider_filter: ProviderFilter::All,
        }
    }

    pub fn accounts_normal(provider_filter: ProviderFilter) -> Self {
        Self {
            workspace: Workspace::Accounts,
            mode: Mode::Normal,
            provider_filter,
        }
    }
}

fn append_char(input: &str, c: char) -> String {
    let mut next = input.to_string();
    next.push(c);
    next
}

fn pop_char(input: &str) -> String {
    let mut next = input.to_string();
    next.pop();
    next
}

fn next_edit_field(field: AccountsEditField, reverse: bool) -> AccountsEditField {
    match (field, reverse) {
        (AccountsEditField::Label, false) => AccountsEditField::Notes,
        (AccountsEditField::Notes, false) => AccountsEditField::Enabled,
        (AccountsEditField::Enabled, false) => AccountsEditField::Label,
        (AccountsEditField::Label, true) => AccountsEditField::Enabled,
        (AccountsEditField::Notes, true) => AccountsEditField::Label,
        (AccountsEditField::Enabled, true) => AccountsEditField::Notes,
    }
}

fn edit_buffer(draft: &ProfileDraft, field: AccountsEditField) -> &str {
    match field {
        AccountsEditField::Label => &draft.label,
        AccountsEditField::Notes => &draft.notes,
        AccountsEditField::Enabled => "",
    }
}

fn set_edit_field(draft: &ProfileDraft, field: AccountsEditField) -> SemanticAction {
    SemanticAction::Accounts(crate::app::AccountsAction::SetMode(AccountsMode::Editing {
        draft: draft.clone(),
        field,
    }))
}

fn overview_filter_action(input: String) -> SemanticAction {
    SemanticAction::Overview(crate::app::OverviewAction::UpdateFilter(input))
}

fn accounts_filter_action(input: String) -> SemanticAction {
    SemanticAction::Accounts(crate::app::AccountsAction::UpdateFilter(input))
}

#[cfg(test)]
mod tests {
    use super::{Action, Context, Mode, RuntimeAction};
    use crate::app::{
        AccountsAction, AccountsConfirmAction, AccountsEditField, Action as SemanticAction,
        OverviewAction, ProfileDraft, SystemAction,
    };
    use crate::domain::accounts::ProviderFilter;
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

    fn map(key: KeyEvent, context: &Context) -> Option<Action> {
        super::keymap::action_for_key(key, context)
    }

    fn app(action: SemanticAction) -> Action {
        Action::App(Box::new(action))
    }

    #[test]
    fn overview_normal_maps_navigation_and_filter_to_semantic_actions() {
        let context = Context::overview_normal();

        assert_eq!(
            map(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE), &context),
            Some(app(SemanticAction::Overview(OverviewAction::SelectNext)))
        );
        assert_eq!(
            map(
                KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE),
                &context
            ),
            Some(app(
                SemanticAction::Overview(OverviewAction::CycleSortMode,)
            ))
        );
        assert_eq!(
            map(
                KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE),
                &context
            ),
            Some(app(SemanticAction::Overview(
                OverviewAction::StartFiltering,
            )))
        );
    }

    #[test]
    fn accounts_normal_maps_filter_cycle_and_confirm_request_by_workspace() {
        let context = Context::accounts_normal(ProviderFilter::Codex);

        assert_eq!(
            map(
                KeyEvent::new(KeyCode::Char('f'), KeyModifiers::NONE),
                &context
            ),
            Some(app(SemanticAction::Accounts(
                AccountsAction::SetProviderFilter(ProviderFilter::Claude),
            )))
        );
        assert_eq!(
            map(
                KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE),
                &context
            ),
            Some(app(SemanticAction::Accounts(
                AccountsAction::StartFiltering,
            )))
        );
        assert_eq!(
            map(
                KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE),
                &context
            ),
            Some(app(SemanticAction::Accounts(
                AccountsAction::RequestConfirmation(AccountsConfirmAction::DeleteProfile),
            )))
        );
    }

    #[test]
    fn overview_filter_mode_maps_chars_backspace_and_confirm_semantically() {
        let context = Context {
            workspace: crate::app::Workspace::Overview,
            mode: Mode::OverviewFilter {
                input: "ab".to_string(),
            },
            provider_filter: ProviderFilter::All,
        };

        assert_eq!(
            map(
                KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE),
                &context
            ),
            Some(app(SemanticAction::Overview(OverviewAction::UpdateFilter(
                "abc".to_string()
            ),)))
        );
        assert_eq!(
            map(
                KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE),
                &context
            ),
            Some(app(SemanticAction::Overview(OverviewAction::UpdateFilter(
                "a".to_string()
            ),)))
        );
        assert_eq!(
            map(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), &context),
            Some(app(
                SemanticAction::Overview(OverviewAction::ConfirmFilter,)
            ))
        );
    }

    #[test]
    fn overview_rename_mode_maps_to_semantic_rename_actions() {
        let context = Context {
            workspace: crate::app::Workspace::Overview,
            mode: Mode::OverviewRename {
                input: "cod".to_string(),
            },
            provider_filter: ProviderFilter::All,
        };

        assert_eq!(
            map(
                KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE),
                &context
            ),
            Some(app(SemanticAction::Overview(
                OverviewAction::UpdateRenameInput("code".to_string()),
            )))
        );
        assert_eq!(
            map(
                KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE),
                &context
            ),
            Some(app(SemanticAction::Overview(
                OverviewAction::UpdateRenameInput("co".to_string()),
            )))
        );
        assert_eq!(
            map(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), &context),
            Some(app(
                SemanticAction::Overview(OverviewAction::ConfirmRename,)
            ))
        );
    }

    #[test]
    fn accounts_edit_mode_maps_to_semantic_edit_actions() {
        let context = Context {
            workspace: crate::app::Workspace::Accounts,
            mode: Mode::AccountsEdit {
                draft: ProfileDraft {
                    label: "Label".to_string(),
                    email: None,
                    plan: None,
                    notes: "notes".to_string(),
                    enabled: true,
                },
                field: AccountsEditField::Label,
            },
            provider_filter: ProviderFilter::All,
        };

        assert_eq!(
            map(
                KeyEvent::new(KeyCode::Char('!'), KeyModifiers::NONE),
                &context
            ),
            Some(app(SemanticAction::Accounts(
                AccountsAction::UpdateEditBuffer("Label!".to_string()),
            )))
        );
        assert_eq!(
            map(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE), &context),
            Some(app(SemanticAction::Accounts(AccountsAction::SetMode(
                crate::app::AccountsMode::Editing {
                    draft: ProfileDraft {
                        label: "Label".to_string(),
                        email: None,
                        plan: None,
                        notes: "notes".to_string(),
                        enabled: true,
                    },
                    field: AccountsEditField::Notes,
                },
            ))))
        );
        assert_eq!(
            map(
                KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE),
                &context
            ),
            Some(app(
                SemanticAction::Accounts(AccountsAction::ToggleEnabled,)
            ))
        );
    }

    #[test]
    fn help_mode_maps_to_help_scrolling_and_dismissal() {
        let context = Context {
            workspace: crate::app::Workspace::Overview,
            mode: Mode::Help,
            provider_filter: ProviderFilter::All,
        };

        assert_eq!(
            map(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE), &context),
            Some(app(SemanticAction::System(SystemAction::ScrollHelpDown,)))
        );
        assert_eq!(
            map(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE), &context),
            Some(Action::Runtime(RuntimeAction::DismissCurrentMode))
        );
    }

    #[test]
    fn global_quit_and_toggle_help_are_available_from_normal_mode() {
        let context = Context::overview_normal();

        assert_eq!(
            map(
                KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE),
                &context
            ),
            Some(app(SemanticAction::QuitRequested))
        );
        assert_eq!(
            map(
                KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE),
                &context
            ),
            Some(Action::Runtime(RuntimeAction::ToggleHelp))
        );
    }

    #[test]
    fn non_press_keys_are_ignored() {
        let context = Context::overview_normal();
        let mut key = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);
        key.kind = KeyEventKind::Release;
        assert_eq!(map(key, &context), None);
    }
}
