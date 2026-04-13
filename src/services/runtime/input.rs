use crate::app::{AccountsAction, AccountsConfirmAction, ActiveTab, Effect};
use crate::input::{self, Context, RuntimeAction};

use super::quota;
use super::RuntimeSession;

pub(super) fn input_context(session: &RuntimeSession) -> Context {
    let workspace = session.active_tab();
    let mode = if session.show_help() {
        input::Mode::Help
    } else if workspace == ActiveTab::Overview {
        match session.root().overview().mode() {
            crate::app::OverviewMode::Renaming { input, .. } => input::Mode::OverviewRename {
                input: input.clone(),
            },
            crate::app::OverviewMode::Filtering { input } => input::Mode::OverviewFilter {
                input: input.clone(),
            },
            crate::app::OverviewMode::Normal => input::Mode::Normal,
        }
    } else if workspace == ActiveTab::Accounts {
        match session.root().accounts().mode() {
            crate::app::AccountsMode::Filtering { input } => input::Mode::AccountsFilter {
                input: input.clone(),
            },
            crate::app::AccountsMode::Editing { draft, field } => input::Mode::AccountsEdit {
                draft: draft.clone(),
                field: field.to_owned(),
            },
            crate::app::AccountsMode::Confirming { .. } => input::Mode::AccountsConfirm,
            crate::app::AccountsMode::Normal => input::Mode::Normal,
        }
    } else {
        input::Mode::Normal
    };

    Context {
        workspace,
        mode,
        provider_filter: session.root().accounts().provider_filter(),
    }
}

pub(super) async fn handle_semantic_input_action(
    session: &mut RuntimeSession,
    action: crate::app::Action,
) -> anyhow::Result<bool> {
    Ok(session.dispatch_action(action).await)
}

pub(super) async fn handle_runtime_input_action(
    session: &mut RuntimeSession,
    effect_tx: &tokio::sync::mpsc::Sender<crate::app::EffectResult>,
    action: RuntimeAction,
) -> anyhow::Result<bool> {
    match action {
        RuntimeAction::ToggleHelp => session.toggle_help(),
        RuntimeAction::DismissCurrentMode => match session.active_tab() {
            ActiveTab::Overview => session.dismiss_overlay(),
            ActiveTab::Accounts => session.dismiss_accounts_overlay(),
        },
        RuntimeAction::DismissSelectedAgent => match session.active_tab() {
            ActiveTab::Overview => session.remove_selected(),
            ActiveTab::Accounts => {
                if session.selected_account().is_some() {
                    session
                        .run_accounts_reducer_action(AccountsAction::RequestConfirmation(
                            AccountsConfirmAction::DeleteProfile,
                        ))
                        .await;
                }
            }
        },
        RuntimeAction::DismissExitedAgents => {
            if session.active_tab() == ActiveTab::Overview {
                session.remove_exited();
            }
        }
        RuntimeAction::StartOverviewRename => {
            if session.active_tab() == ActiveTab::Overview {
                session.start_rename();
            }
        }
        RuntimeAction::ImportCodexProfile => {
            if session.active_tab() == ActiveTab::Accounts {
                session
                    .run_accounts_effect(Effect::ImportCodexProfile)
                    .await;
            }
        }
        RuntimeAction::ImportClaudeProfile => {
            if session.active_tab() == ActiveTab::Accounts {
                session
                    .run_accounts_effect(Effect::ImportClaudeProfile)
                    .await;
            }
        }
        RuntimeAction::RequestWriteProfile => {
            if session.active_tab() == ActiveTab::Accounts && session.selected_account().is_some() {
                session
                    .run_accounts_reducer_action(AccountsAction::RequestConfirmation(
                        AccountsConfirmAction::WriteProfileToHome,
                    ))
                    .await;
            }
        }
        RuntimeAction::StartAccountsEdit => {
            if session.active_tab() == ActiveTab::Accounts {
                if let Some((provider, profile_id)) = session.selected_account() {
                    session
                        .run_accounts_effect(Effect::LoadProfileDraft {
                            provider,
                            profile_id: profile_id.to_string(),
                        })
                        .await;
                }
            }
        }
        RuntimeAction::RefreshSelectedQuota => {
            if session.active_tab() == ActiveTab::Accounts {
                quota::spawn_manual_quota_refresh(session, effect_tx);
            }
        }
    }
    Ok(true)
}
