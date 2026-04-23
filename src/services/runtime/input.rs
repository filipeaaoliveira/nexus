use crate::app::{AccountsAction, AccountsConfirmAction, Effect, Workspace};
use crate::input::{self, Context, RuntimeAction};
use nexus_domain::accounts::ProfileId;

use super::quota;
use super::RuntimeSession;

pub(super) fn input_context(session: &RuntimeSession) -> Context {
    let workspace = session.active_tab();
    let mode = if session.show_help() {
        input::Mode::Help
    } else if workspace == Workspace::Overview {
        match session.root().overview().mode() {
            crate::app::OverviewMode::Renaming { input, .. } => input::Mode::OverviewRename {
                input: input.clone(),
            },
            crate::app::OverviewMode::Filtering { input } => input::Mode::OverviewFilter {
                input: input.clone(),
            },
            crate::app::OverviewMode::Normal => input::Mode::Normal,
        }
    } else if workspace == Workspace::Accounts {
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
            Workspace::Overview => session.dismiss_overlay(),
            Workspace::Accounts => session.dismiss_accounts_overlay(),
        },
        RuntimeAction::DismissSelectedAgent => match session.active_tab() {
            Workspace::Overview => session.remove_selected(),
            Workspace::Accounts => {
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
            if session.active_tab() == Workspace::Overview {
                session.remove_exited();
            }
        }
        RuntimeAction::StartOverviewRename => {
            if session.active_tab() == Workspace::Overview {
                session.start_rename();
            }
        }
        RuntimeAction::ImportCodexProfile => {
            if session.active_tab() == Workspace::Accounts {
                session
                    .run_accounts_effect(Effect::ImportProfile {
                        provider: nexus_domain::accounts::Provider::Codex,
                    })
                    .await;
            }
        }
        RuntimeAction::ImportClaudeProfile => {
            if session.active_tab() == Workspace::Accounts {
                session
                    .run_accounts_effect(Effect::ImportProfile {
                        provider: nexus_domain::accounts::Provider::Claude,
                    })
                    .await;
            }
        }
        RuntimeAction::RequestWriteProfile => {
            if session.active_tab() == Workspace::Accounts && session.selected_account().is_some() {
                session
                    .run_accounts_reducer_action(AccountsAction::RequestConfirmation(
                        AccountsConfirmAction::WriteProfileToHome,
                    ))
                    .await;
            }
        }
        RuntimeAction::StartAccountsEdit => {
            if session.active_tab() == Workspace::Accounts {
                if let Some((provider, profile_id)) = session.selected_account() {
                    session
                        .run_accounts_effect(Effect::LoadEditableProfile {
                            provider,
                            profile_id: ProfileId::new_unchecked(profile_id),
                        })
                        .await;
                }
            }
        }
        RuntimeAction::RefreshSelectedQuota => {
            if session.active_tab() == Workspace::Accounts {
                quota::spawn_manual_quota_refresh(session, effect_tx);
            }
        }
    }
    Ok(true)
}
