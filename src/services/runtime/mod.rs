pub mod logging;
pub mod terminal;

use std::time::Duration;

use crate::app::dispatch::effects::dispatch_effect;
use crate::app::{ActiveTab, AppState, Effect, EffectResult, RefreshOrigin};
use crate::config::Config;
use crate::detect;
use crate::event::AgentEvent;
use crate::input;

pub async fn run(config: Config) -> anyhow::Result<()> {
    logging::initialize_tracing(&config)?;
    terminal::ensure_interactive_terminal()?;
    terminal::install_panic_restore_hook();

    let mut rx = detect::spawn_detectors(&config);
    let mut state = AppState::new(config);
    state.initialize_accounts().await;

    let mut terminal = terminal::init_terminal()?;
    let result = run_loop(&mut terminal, &mut state, &mut rx).await;

    ratatui::restore();
    result
}

async fn run_loop(
    terminal: &mut ratatui::DefaultTerminal,
    state: &mut AppState,
    rx: &mut tokio::sync::mpsc::Receiver<AgentEvent>,
) -> anyhow::Result<()> {
    let (effect_tx, mut effect_rx) = tokio::sync::mpsc::channel::<EffectResult>(16);

    let config = state.config.clone();

    tokio::spawn(poll_quota_loop(
        config.codex_profiles_dir.clone(),
        effect_tx.clone(),
        {
            let config = config.clone();
            move |pid, _profile_dir| {
                let config = config.clone();
                async move {
                    dispatch_effect(
                        &config,
                        Effect::RefreshCodexQuota {
                            profile_id: pid,
                            origin: RefreshOrigin::Background,
                        },
                    )
                    .await
                }
            }
        },
    ));

    tokio::spawn(poll_quota_loop(
        config.claude_profiles_dir.clone(),
        effect_tx.clone(),
        {
            let config = config.clone();
            move |pid, _profile_dir| {
                let config = config.clone();
                async move {
                    dispatch_effect(
                        &config,
                        Effect::RefreshClaudeQuota {
                            profile_id: pid,
                            origin: RefreshOrigin::Background,
                        },
                    )
                    .await
                }
            }
        },
    ));

    #[cfg(unix)]
    let (sigterm_tx, mut sigterm_rx) = tokio::sync::mpsc::channel::<()>(1);
    #[cfg(unix)]
    tokio::spawn(async move {
        use tokio::signal::unix::{signal, SignalKind};
        if let Ok(mut stream) = signal(SignalKind::terminate()) {
            if stream.recv().await.is_some() {
                let _ = sigterm_tx.send(()).await;
            }
        }
    });

    loop {
        #[cfg(unix)]
        if sigterm_rx.try_recv().is_ok() {
            break;
        }

        while let Ok(event) = rx.try_recv() {
            state.handle_event(event);
        }

        while let Ok(result) = effect_rx.try_recv() {
            state.apply_accounts_effect_result(result);
        }

        state.clear_expired_accounts_status();

        terminal.draw(|frame| {
            crate::ui::render(frame, state);
        })?;

        let context = input_context(state);
        match input::poll_action(Duration::from_millis(100), &context)? {
            Some(input::Action::App(action)) => {
                if !handle_semantic_input_action(state, &effect_tx, *action).await? {
                    break;
                }
            }
            Some(input::Action::Runtime(action)) => {
                if !handle_runtime_input_action(state, &effect_tx, action).await? {
                    break;
                }
            }
            Some(input::Action::Tick) | None => {}
        }
    }
    Ok(())
}

fn input_context(state: &AppState) -> input::Context {
    let workspace = state.active_tab();
    let mode = if state.show_help() {
        input::Mode::Help
    } else if workspace == ActiveTab::Overview {
        match &state.root.overview.mode {
            crate::app::OverviewMode::Renaming { input, .. } => {
                input::Mode::OverviewRename { input: input.clone() }
            }
            crate::app::OverviewMode::Filtering { input } => {
                input::Mode::OverviewFilter { input: input.clone() }
            }
            crate::app::OverviewMode::Normal => input::Mode::Normal,
        }
    } else if workspace == ActiveTab::Accounts {
        match &state.root.accounts.mode {
            crate::app::AccountsMode::Filtering { input } => {
                input::Mode::AccountsFilter { input: input.clone() }
            }
            crate::app::AccountsMode::Editing { draft, field } => input::Mode::AccountsEdit {
                draft: draft.clone(),
                field: *field,
            },
            crate::app::AccountsMode::Confirming { .. } => input::Mode::AccountsConfirm,
            crate::app::AccountsMode::Normal => input::Mode::Normal,
        }
    } else {
        input::Mode::Normal
    };

    input::Context {
        workspace,
        mode,
        provider_filter: state.root.accounts.provider_filter,
    }
}

async fn handle_semantic_input_action(
    state: &mut AppState,
    _effect_tx: &tokio::sync::mpsc::Sender<EffectResult>,
    action: crate::app::Action,
) -> anyhow::Result<bool> {
    match action {
        crate::app::Action::QuitRequested => return Ok(false),
        crate::app::Action::Workspace(crate::app::WorkspaceAction::Next) => state.next_tab(),
        crate::app::Action::Overview(action) => state.run_overview_reducer_action(action),
        crate::app::Action::Accounts(action) => state.run_accounts_reducer_action(action).await,
        crate::app::Action::System(action) => match action {
            crate::app::SystemAction::ScrollHelpDown => state.scroll_help_down(),
            crate::app::SystemAction::ScrollHelpUp => state.scroll_help_up(),
            _ => {}
        },
        _ => {}
    }
    Ok(true)
}

async fn handle_runtime_input_action(
    state: &mut AppState,
    effect_tx: &tokio::sync::mpsc::Sender<EffectResult>,
    action: input::RuntimeAction,
) -> anyhow::Result<bool> {
    match action {
        input::RuntimeAction::ToggleHelp => state.toggle_help(),
        input::RuntimeAction::DismissCurrentMode => match state.active_tab() {
            ActiveTab::Overview => state.dismiss_overlay(),
            ActiveTab::Accounts => state.dismiss_accounts_overlay(),
        },
        input::RuntimeAction::DismissSelectedAgent => match state.active_tab() {
            ActiveTab::Overview => state.remove_selected(),
            ActiveTab::Accounts => {
                if state.selected_account().is_some() {
                    state
                        .run_accounts_reducer_action(crate::app::AccountsAction::RequestConfirmation(
                            crate::app::AccountsConfirmAction::DeleteProfile,
                        ))
                        .await;
                }
            }
        },
        input::RuntimeAction::DismissExitedAgents => {
            if state.active_tab() == ActiveTab::Overview {
                state.remove_exited();
            }
        }
        input::RuntimeAction::StartOverviewRename => {
            if state.active_tab() == ActiveTab::Overview {
                state.start_rename();
            }
        }
        input::RuntimeAction::ImportCodexProfile => {
            if state.active_tab() == ActiveTab::Accounts {
                state.run_accounts_effect(Effect::ImportCodexProfile).await;
            }
        }
        input::RuntimeAction::ImportClaudeProfile => {
            if state.active_tab() == ActiveTab::Accounts {
                state.run_accounts_effect(Effect::ImportClaudeProfile).await;
            }
        }
        input::RuntimeAction::RequestWriteProfile => {
            if state.active_tab() == ActiveTab::Accounts && state.selected_account().is_some() {
                state
                    .run_accounts_reducer_action(crate::app::AccountsAction::RequestConfirmation(
                        crate::app::AccountsConfirmAction::WriteProfileToHome,
                    ))
                    .await;
            }
        }
        input::RuntimeAction::StartAccountsEdit => {
            if state.active_tab() == ActiveTab::Accounts {
                if let Some((provider, profile_id)) = state.selected_account() {
                    state
                        .run_accounts_effect(Effect::LoadProfileDraft {
                            provider,
                            profile_id: profile_id.to_string(),
                        })
                        .await;
                }
            }
        }
        input::RuntimeAction::RefreshSelectedQuota => {
            if state.active_tab() == ActiveTab::Accounts {
                spawn_manual_quota_refresh(state, effect_tx);
            }
        }
    }
    Ok(true)
}

fn spawn_manual_quota_refresh(
    state: &mut AppState,
    effect_tx: &tokio::sync::mpsc::Sender<EffectResult>,
) {
    match state.selected_account() {
        Some((crate::domain::accounts::Provider::Claude, profile_id)) => {
            let profile_id = profile_id.to_string();
            state
                .begin_manual_quota_refresh(crate::domain::accounts::Provider::Claude, &profile_id);
            let tx = effect_tx.clone();
            let config = state.config.clone();
            tokio::spawn(async move {
                let result = dispatch_effect(
                    &config,
                    Effect::RefreshClaudeQuota {
                        profile_id,
                        origin: RefreshOrigin::Manual,
                    },
                )
                .await;
                let _ = tx.send(result).await;
            });
        }
        Some((crate::domain::accounts::Provider::Codex, profile_id)) => {
            let profile_id = profile_id.to_string();
            state.begin_manual_quota_refresh(crate::domain::accounts::Provider::Codex, &profile_id);
            let tx = effect_tx.clone();
            let config = state.config.clone();
            tokio::spawn(async move {
                let result = dispatch_effect(
                    &config,
                    Effect::RefreshCodexQuota {
                        profile_id,
                        origin: RefreshOrigin::Manual,
                    },
                )
                .await;
                let _ = tx.send(result).await;
            });
        }
        None => {}
    }
}

async fn poll_quota_loop<F, Fut>(
    profiles_dir: std::path::PathBuf,
    tx: tokio::sync::mpsc::Sender<EffectResult>,
    fetch_profile: F,
) where
    F: Fn(String, std::path::PathBuf) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = EffectResult> + Send + 'static,
{
    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(30));

    loop {
        interval.tick().await;
        refresh_all_profiles_once(&profiles_dir, &tx, &fetch_profile).await;
    }
}

async fn refresh_all_profiles_once<F, Fut>(
    profiles_dir: &std::path::Path,
    tx: &tokio::sync::mpsc::Sender<EffectResult>,
    fetch_profile: &F,
) where
    F: Fn(String, std::path::PathBuf) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = EffectResult> + Send + 'static,
{
    let profiles: Vec<String> = match std::fs::read_dir(profiles_dir) {
        Ok(entries) => entries
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_dir())
            .filter_map(|e| e.file_name().into_string().ok())
            .collect(),
        Err(_) => return,
    };

    let mut set = tokio::task::JoinSet::new();
    for profile_id in profiles {
        let profile_dir = profiles_dir.join(&profile_id);
        set.spawn(fetch_profile(profile_id, profile_dir));
    }

    while let Some(Ok(result)) = set.join_next().await {
        let _ = tx.send(result).await;
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::Duration;

    use chrono::Utc;
    use tempfile::tempdir;
    use tokio::sync::mpsc;

    use super::poll_quota_loop;
    use crate::app::RefreshOrigin;
    use crate::codex_profiles::quota::{Confidence, LimitState, QuotaSnapshot};
    use crate::{Effect, EffectResult};

    fn test_snapshot() -> QuotaSnapshot {
        QuotaSnapshot::legacy_fallback(
            Utc::now(),
            Confidence::Exact,
            LimitState::Available,
            None,
            Some(25),
            LimitState::Available,
            None,
            Some(40),
            "5h=25%; weekly=40%".to_string(),
        )
    }

    #[tokio::test]
    async fn poll_quota_loop_refreshes_immediately_before_first_interval() {
        let temp = tempdir().expect("tempdir");
        let profile_dir = temp.path().join("profile-01");
        fs::create_dir_all(&profile_dir).expect("profile dir");

        let (tx, mut rx) = mpsc::channel(4);
        let handle = tokio::spawn(poll_quota_loop(
            temp.path().to_path_buf(),
            tx,
            |pid, _| async move {
                EffectResult::CodexQuotaRefreshed {
                    profile_id: pid,
                    quota: test_snapshot(),
                    origin: RefreshOrigin::Background,
                }
            },
        ));

        let result = tokio::time::timeout(Duration::from_millis(100), rx.recv())
            .await
            .expect("quota refresh should happen immediately")
            .expect("quota message");

        match result {
            EffectResult::CodexQuotaRefreshed {
                profile_id,
                quota,
                origin,
            } => {
                assert_eq!(profile_id, "profile-01");
                assert_eq!(quota.summary, test_snapshot().summary);
                assert_eq!(origin, RefreshOrigin::Background);
            }
            other => panic!("unexpected result: {other:?}"),
        }

        handle.abort();
    }

    #[tokio::test]
    async fn poll_quota_loop_forwards_failed_refreshes() {
        let temp = tempdir().expect("tempdir");
        let profile_dir = temp.path().join("profile-01");
        fs::create_dir_all(&profile_dir).expect("profile dir");

        let (tx, mut rx) = mpsc::channel(4);
        let handle = tokio::spawn(poll_quota_loop(
            temp.path().to_path_buf(),
            tx,
            |pid, _| async move {
                EffectResult::Failed {
                    effect: Effect::RefreshCodexQuota {
                        profile_id: pid,
                        origin: RefreshOrigin::Background,
                    },
                    message: "failed refresh".to_string(),
                }
            },
        ));

        let result = tokio::time::timeout(Duration::from_millis(100), rx.recv())
            .await
            .expect("failed background refresh should still be forwarded")
            .expect("quota message");

        match result {
            EffectResult::Failed {
                effect: Effect::RefreshCodexQuota { profile_id, origin },
                ..
            } => {
                assert_eq!(profile_id, "profile-01");
                assert_eq!(origin, RefreshOrigin::Background);
            }
            other => panic!("unexpected result: {other:?}"),
        }

        handle.abort();
    }
}
