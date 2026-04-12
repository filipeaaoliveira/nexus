use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use nexus::app::{
    AccountsAction, AccountsConfirmAction, AccountsEditField, Action as SemanticAction,
    OverviewAction, ProfileDraft, WorkspaceAction,
};
use nexus::domain::accounts::ProviderFilter;
use nexus::input::{action_for_key, Action, Context, Mode, RuntimeAction};

fn app(action: SemanticAction) -> Action {
    Action::App(Box::new(action))
}

#[test]
fn normal_overview_keys_map_to_workspace_semantics() {
    let context = Context::overview_normal();

    assert_eq!(
        action_for_key(
            KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE),
            &context
        ),
        Some(app(SemanticAction::QuitRequested))
    );
    assert_eq!(
        action_for_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE), &context),
        Some(app(SemanticAction::Workspace(WorkspaceAction::Next)))
    );
    assert_eq!(
        action_for_key(
            KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE),
            &context
        ),
        Some(Action::Runtime(RuntimeAction::ToggleHelp))
    );
    assert_eq!(
        action_for_key(
            KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE),
            &context
        ),
        None
    );
}

#[test]
fn overview_filter_and_rename_modes_keep_text_entry_semantic() {
    let filter = Context {
        workspace: nexus::app::Workspace::Overview,
        mode: Mode::OverviewFilter {
            input: "ab".to_string(),
        },
        provider_filter: ProviderFilter::All,
    };
    assert_eq!(
        action_for_key(
            KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE),
            &filter
        ),
        Some(app(SemanticAction::Overview(OverviewAction::UpdateFilter(
            "aba".to_string()
        ),)))
    );
    assert_eq!(
        action_for_key(
            KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE),
            &filter
        ),
        Some(app(SemanticAction::Overview(OverviewAction::UpdateFilter(
            "a".to_string()
        ),)))
    );
    assert_eq!(
        action_for_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), &filter),
        Some(app(
            SemanticAction::Overview(OverviewAction::ConfirmFilter,)
        ))
    );
    assert_eq!(
        action_for_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE), &filter),
        Some(app(SemanticAction::Overview(OverviewAction::CancelMode,)))
    );

    let rename = Context {
        workspace: nexus::app::Workspace::Overview,
        mode: Mode::OverviewRename {
            input: "co".to_string(),
        },
        provider_filter: ProviderFilter::All,
    };
    assert_eq!(
        action_for_key(
            KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE),
            &rename
        ),
        Some(app(SemanticAction::Overview(
            OverviewAction::UpdateRenameInput("cox".to_string()),
        )))
    );
    assert_eq!(
        action_for_key(
            KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE),
            &rename
        ),
        Some(app(SemanticAction::Overview(
            OverviewAction::UpdateRenameInput("c".to_string()),
        )))
    );
    assert_eq!(
        action_for_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), &rename),
        Some(app(
            SemanticAction::Overview(OverviewAction::ConfirmRename,)
        ))
    );
}

#[test]
fn accounts_modes_are_explicit_and_workspace_specific() {
    let filter = Context {
        workspace: nexus::app::Workspace::Accounts,
        mode: Mode::AccountsFilter {
            input: "x".to_string(),
        },
        provider_filter: ProviderFilter::All,
    };
    assert_eq!(
        action_for_key(
            KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE),
            &filter
        ),
        Some(app(SemanticAction::Accounts(AccountsAction::UpdateFilter(
            "xa".to_string()
        ),)))
    );

    let edit = Context {
        workspace: nexus::app::Workspace::Accounts,
        mode: Mode::AccountsEdit {
            draft: ProfileDraft {
                label: "Lab".to_string(),
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
        action_for_key(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE), &edit),
        Some(app(SemanticAction::Accounts(
            AccountsAction::UpdateEditBuffer("Labe".to_string()),
        )))
    );
    assert_eq!(
        action_for_key(KeyEvent::new(KeyCode::BackTab, KeyModifiers::NONE), &edit),
        Some(app(SemanticAction::Accounts(AccountsAction::SetMode(
            nexus::app::AccountsMode::Editing {
                draft: ProfileDraft {
                    label: "Lab".to_string(),
                    email: None,
                    plan: None,
                    notes: "notes".to_string(),
                    enabled: true,
                },
                field: AccountsEditField::Enabled,
            },
        ))))
    );

    let confirm = Context {
        workspace: nexus::app::Workspace::Accounts,
        mode: Mode::AccountsConfirm,
        provider_filter: ProviderFilter::All,
    };
    assert_eq!(
        action_for_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), &confirm),
        Some(app(SemanticAction::Accounts(AccountsAction::Confirm,)))
    );
    assert_eq!(
        action_for_key(
            KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE),
            &Context::accounts_normal(ProviderFilter::All),
        ),
        None
    );
}

#[test]
fn accounts_normal_has_runtime_only_commands_for_side_effectful_actions() {
    let context = Context::accounts_normal(ProviderFilter::Codex);

    assert_eq!(
        action_for_key(
            KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE),
            &context
        ),
        Some(Action::Runtime(RuntimeAction::ImportCodexProfile))
    );
    assert_eq!(
        action_for_key(
            KeyEvent::new(KeyCode::Char('I'), KeyModifiers::NONE),
            &context
        ),
        Some(Action::Runtime(RuntimeAction::ImportClaudeProfile))
    );
    assert_eq!(
        action_for_key(
            KeyEvent::new(KeyCode::Char('w'), KeyModifiers::NONE),
            &context
        ),
        Some(Action::Runtime(RuntimeAction::RequestWriteProfile))
    );
    assert_eq!(
        action_for_key(
            KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE),
            &context
        ),
        Some(Action::Runtime(RuntimeAction::StartAccountsEdit))
    );
    assert_eq!(
        action_for_key(
            KeyEvent::new(KeyCode::Char('R'), KeyModifiers::NONE),
            &context
        ),
        Some(Action::Runtime(RuntimeAction::RefreshSelectedQuota))
    );
    assert_eq!(
        action_for_key(
            KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE),
            &context
        ),
        Some(app(SemanticAction::Accounts(
            AccountsAction::RequestConfirmation(AccountsConfirmAction::DeleteProfile),
        )))
    );
}
