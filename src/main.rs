use clap::Parser;
use nexus::app::ActiveTab;
use nexus::app::AppState;
use nexus::claude_profiles::auth::ClaudeAuth;
use nexus::claude_profiles::quota::{fetch_claude_quota, refresh_token_if_needed};
use nexus::codex_profiles::quota::fetch_quota_from_api;
use nexus::codex_profiles::ProfileAuth;
use nexus::config::{Cli, Config};
use nexus::detect;
use nexus::event::AgentEvent;
use nexus::input;
use std::io::{stdout, IsTerminal};
use std::time::Duration;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let config = Config::from_cli(&cli);
    initialize_tracing(&config)?;

    // Let clap handle non-interactive help/version output before enforcing TUI startup.
    if !std::io::stdin().is_terminal() {
        anyhow::bail!("stdin is not a terminal — nexus must run in an interactive terminal");
    }
    if !stdout().is_terminal() {
        anyhow::bail!("stdout is not a terminal — nexus must run in an interactive terminal");
    }

    // Restore terminal state on panic so the user's shell is not left in raw mode.
    let prev_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        ratatui::restore();
        prev_hook(info);
    }));

    migrate_data_dir_if_needed(&config);

    let mut rx = detect::spawn_detectors(&config);
    let mut state = AppState::new(config);

    let mut terminal = ratatui::init();
    // Clear once on entry so stale alternate-screen content from a previous
    // session does not show through before the first draw() call completes.
    terminal.clear()?;

    let result = run_loop(&mut terminal, &mut state, &mut rx).await;

    ratatui::restore();
    result
}

fn initialize_tracing(config: &Config) -> anyhow::Result<()> {
    // Initialize tracing to a file so debug output does not corrupt the TUI.
    // Use RUST_LOG to control level (default: info).
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("nexus=info"));
    if let Some(parent) = config.log_file.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let log_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&config.log_file)?;
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(log_file)
        .with_ansi(false)
        .init();
    Ok(())
}

/// One-time migration: if the old `ai-overview` data directory exists and the
/// new `nexus` directory does not, rename it in place so existing profiles and
/// quota cache are preserved across the rename.
fn migrate_data_dir_if_needed(config: &Config) {
    // Codex management dir: ~/.local/share/nexus/codex
    // Old path:             ~/.local/share/ai-overview/codex
    if let Some(new_parent) = config.codex_management_dir.parent() {
        // new_parent = ~/.local/share/nexus
        if let Some(share_dir) = new_parent.parent() {
            let old_parent = share_dir.join("ai-overview");
            if old_parent.exists() && !new_parent.exists() {
                match std::fs::rename(&old_parent, new_parent) {
                    Ok(()) => tracing::info!(
                        "migrated data directory from {} to {}",
                        old_parent.display(),
                        new_parent.display()
                    ),
                    Err(e) => tracing::warn!(
                        "could not migrate {} to {}: {e}",
                        old_parent.display(),
                        new_parent.display()
                    ),
                }
            }
        }
    }

    // Log file: ~/.cache/nexus/nexus.log
    // Old path: ~/.cache/ai-overview/ai-overview.log
    if let Some(new_log_dir) = config.log_file.parent() {
        if let Some(cache_dir) = new_log_dir.parent() {
            let old_log_dir = cache_dir.join("ai-overview");
            if old_log_dir.exists() && !new_log_dir.exists() {
                match std::fs::rename(&old_log_dir, new_log_dir) {
                    Ok(()) => tracing::info!(
                        "migrated log directory from {} to {}",
                        old_log_dir.display(),
                        new_log_dir.display()
                    ),
                    Err(e) => tracing::warn!(
                        "could not migrate log dir {} to {}: {e}",
                        old_log_dir.display(),
                        new_log_dir.display()
                    ),
                }
            }
        }
    }
}

async fn run_loop(
    terminal: &mut ratatui::DefaultTerminal,
    state: &mut AppState,
    rx: &mut tokio::sync::mpsc::Receiver<AgentEvent>,
) -> anyhow::Result<()> {
    // Channel for async quota fetch results. Carries Option so a None result
    // (API failure) can still clear the quota_loading flag on the UI.
    let (quota_tx, mut quota_rx) = tokio::sync::mpsc::channel::<(
        String,
        Option<nexus::codex_profiles::quota::QuotaSnapshot>,
    )>(10);
    let (claude_quota_tx, mut claude_quota_rx) = tokio::sync::mpsc::channel::<(
        String,
        Option<nexus::codex_profiles::quota::QuotaSnapshot>,
    )>(10);

    let config = state.config.clone();

    // Fire-and-forget: no cancellation token. When run_loop returns and main
    // exits the tokio runtime drops, which aborts these tasks. In-flight
    // requests are abandoned rather than drained — acceptable for a local TUI.
    tokio::spawn(poll_quota_loop(
        config.codex_profiles_dir.clone(),
        quota_tx.clone(),
        |pid, profile_dir| async move {
            let auth_path = profile_dir.join("auth.json");
            if !auth_path.exists() {
                return (pid, None);
            }
            let Ok(auth) = ProfileAuth::load(&auth_path) else {
                return (pid, None);
            };
            (pid, fetch_quota_from_api(&auth).await)
        },
    ));

    // Fire-and-forget: same design as the Codex loop above.
    tokio::spawn(poll_quota_loop(
        config.claude_profiles_dir.clone(),
        claude_quota_tx.clone(),
        |pid, profile_dir| async move {
            let creds_path = profile_dir.join(".credentials.json");
            if !creds_path.exists() {
                return (pid, None);
            }
            let Ok(auth) = ClaudeAuth::load(&creds_path) else {
                return (pid, None);
            };
            let refreshed = refresh_token_if_needed(auth).await;
            if let Err(e) = refreshed.save(&creds_path) {
                tracing::warn!("failed to persist refreshed Claude token: {}", e);
            }
            let quota = fetch_claude_quota(&refreshed).await;
            (pid, quota)
        },
    ));

    // Shut down cleanly on SIGTERM so ratatui::restore() in main runs and the
    // terminal is not left in raw mode. SIGTERM is a Unix concept; on other
    // platforms the runtime drop on exit provides the same cleanup guarantee.
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

        // Drain all pending events
        while let Ok(event) = rx.try_recv() {
            state.handle_event(event);
        }

        // Check for async quota results (non-blocking).
        // None means the fetch failed — clear quota_loading without updating data.
        while let Ok((profile_id, quota)) = quota_rx.try_recv() {
            if let Some(quota) = quota {
                state.update_codex_quota_for_profile(&profile_id, quota);
            } else {
                state.accounts_tab.quota_loading = false;
            }
        }

        while let Ok((profile_id, quota)) = claude_quota_rx.try_recv() {
            if let Some(quota) = quota {
                state.update_claude_quota_for_profile(&profile_id, quota);
            } else {
                state.accounts_tab.quota_loading = false;
            }
        }

        // Clear any status-bar messages whose 5-second display window has elapsed.
        state.accounts_tab.clear_expired_status();

        terminal.draw(|frame| {
            nexus::ui::render(frame, state);
        })?;

        match input::poll_action(
            Duration::from_millis(100),
            state.active_filter_mode(),
            state.active_rename_mode(),
            state.accounts_tab.edit_mode,
            state.accounts_tab.pending_confirm.is_some(),
        )? {
            Some(input::Action::Quit) => break,
            Some(input::Action::MoveDown) => {
                if state.show_help {
                    state.scroll_help_down();
                } else {
                    match state.active_tab() {
                        ActiveTab::Overview => state.move_selection_down(),
                        ActiveTab::Accounts => state.handle_accounts_move_down(),
                    }
                }
            }
            Some(input::Action::MoveUp) => {
                if state.show_help {
                    state.scroll_help_up();
                } else {
                    match state.active_tab() {
                        ActiveTab::Overview => state.move_selection_up(),
                        ActiveTab::Accounts => state.handle_accounts_move_up(),
                    }
                }
            }
            Some(input::Action::NextTab) => state.next_tab(),
            Some(input::Action::CycleSortMode) => state.cycle_sort_mode(),
            Some(input::Action::ShowHelp) => state.toggle_help(),
            Some(input::Action::RemoveAgent) => match state.active_tab() {
                ActiveTab::Overview => state.remove_selected(),
                ActiveTab::Accounts => {
                    if state.selected_account().is_some() {
                        state.accounts_tab.pending_confirm =
                            Some(nexus::app::ConfirmKind::DeleteProfile);
                    }
                }
            },
            Some(input::Action::RemoveExited) => match state.active_tab() {
                ActiveTab::Overview => state.remove_exited(),
                ActiveTab::Accounts => {}
            },
            Some(input::Action::DismissOverlay) => match state.active_tab() {
                ActiveTab::Overview => state.dismiss_overlay(),
                ActiveTab::Accounts => state.dismiss_accounts_overlay(),
            },
            Some(input::Action::StartFilter) => match state.active_tab() {
                ActiveTab::Overview => state.start_filter(),
                ActiveTab::Accounts => state.start_accounts_filter(),
            },
            Some(input::Action::FilterChar(c)) => match state.active_tab() {
                ActiveTab::Overview => state.filter_push_char(c),
                ActiveTab::Accounts => state.accounts_filter_push_char(c),
            },
            Some(input::Action::FilterBackspace) => match state.active_tab() {
                ActiveTab::Overview => state.filter_pop_char(),
                ActiveTab::Accounts => state.accounts_filter_pop_char(),
            },
            Some(input::Action::FilterConfirm) => match state.active_tab() {
                ActiveTab::Overview => state.confirm_filter(),
                ActiveTab::Accounts => state.confirm_accounts_filter(),
            },
            Some(input::Action::StartRename) => match state.active_tab() {
                ActiveTab::Overview => state.start_rename(),
                ActiveTab::Accounts => {}
            },
            Some(input::Action::AccountsFilterCycle) => {
                if state.active_tab() == ActiveTab::Accounts {
                    state.cycle_accounts_filter();
                }
            }
            Some(input::Action::AccountsImportAuth) => {
                if state.active_tab() == ActiveTab::Accounts {
                    if let Err(e) = state.import_codex_auth() {
                        state.accounts_tab.set_status(e.to_string());
                    }
                }
            }
            Some(input::Action::AccountsImportClaudeAuth) => {
                if state.active_tab() == ActiveTab::Accounts {
                    if let Err(e) = state.import_claude_auth() {
                        state.accounts_tab.set_status(e.to_string());
                    }
                }
            }
            Some(input::Action::AccountsWriteToHome) => {
                if state.active_tab() == ActiveTab::Accounts && state.selected_account().is_some() {
                    state.accounts_tab.pending_confirm = Some(nexus::app::ConfirmKind::WriteToHome);
                }
            }
            Some(input::Action::AccountsConfirm) => {
                if state.active_tab() == ActiveTab::Accounts
                    && state.accounts_tab.pending_confirm.is_some()
                {
                    if let Err(e) = state.execute_pending_confirm() {
                        state.accounts_tab.set_status(e.to_string());
                    }
                }
            }
            Some(input::Action::RenameChar(c)) => match state.active_tab() {
                ActiveTab::Overview => state.rename_push_char(c),
                ActiveTab::Accounts => {}
            },
            Some(input::Action::RenameBackspace) => match state.active_tab() {
                ActiveTab::Overview => state.rename_pop_char(),
                ActiveTab::Accounts => {}
            },
            Some(input::Action::RenameConfirm) => match state.active_tab() {
                ActiveTab::Overview => state.confirm_rename(),
                ActiveTab::Accounts => {}
            },
            Some(input::Action::AccountsEditStart) => {
                if state.active_tab() == ActiveTab::Accounts {
                    state.start_accounts_edit();
                }
            }
            Some(input::Action::AccountsEditChar(c)) => {
                if state.accounts_tab.edit_mode {
                    state.accounts_edit_push_char(c);
                }
            }
            Some(input::Action::AccountsEditBackspace) => {
                if state.accounts_tab.edit_mode {
                    state.accounts_edit_pop_char();
                }
            }
            Some(input::Action::AccountsEditToggleEnabled) => {
                if state.accounts_tab.edit_mode {
                    state.accounts_edit_toggle_enabled();
                }
            }
            Some(input::Action::AccountsEditNextField) => {
                if state.accounts_tab.edit_mode {
                    state.accounts_edit_next_field();
                }
            }
            Some(input::Action::AccountsEditPrevField) => {
                if state.accounts_tab.edit_mode {
                    state.accounts_edit_prev_field();
                }
            }
            Some(input::Action::AccountsEditConfirm) => {
                if state.accounts_tab.edit_mode {
                    if let Err(e) = state.confirm_accounts_edit() {
                        state.accounts_tab.set_status(e.to_string());
                    }
                }
            }
            Some(input::Action::AccountsEditCancel) => {
                if state.accounts_tab.edit_mode {
                    state.dismiss_accounts_edit();
                }
            }
            Some(input::Action::AccountsRefreshQuota) => {
                if state.active_tab() == ActiveTab::Accounts {
                    match state.selected_account() {
                        Some((nexus::app::Provider::Claude, profile_id)) => {
                            let profile_id = profile_id.to_string();
                            let creds_path = state
                                .config
                                .claude_profiles_dir
                                .join(&profile_id)
                                .join(".credentials.json");
                            if creds_path.exists() {
                                if let Ok(auth) = ClaudeAuth::load(&creds_path) {
                                    state.accounts_tab.quota_loading = true;
                                    let tx = claude_quota_tx.clone();
                                    tokio::spawn(async move {
                                        let refreshed = refresh_token_if_needed(auth).await;
                                        if let Err(e) = refreshed.save(&creds_path) {
                                            tracing::warn!(
                                                "failed to persist refreshed Claude token: {}",
                                                e
                                            );
                                        }
                                        let quota = fetch_claude_quota(&refreshed).await;
                                        let _ = tx.send((profile_id, quota)).await;
                                    });
                                }
                            }
                        }
                        Some((nexus::app::Provider::Codex, profile_id)) => {
                            let profile_id = profile_id.to_string();
                            let auth_path = state
                                .config
                                .codex_profiles_dir
                                .join(&profile_id)
                                .join("auth.json");
                            if auth_path.exists() {
                                if let Ok(auth) = ProfileAuth::load(&auth_path) {
                                    state.accounts_tab.quota_loading = true;
                                    let tx = quota_tx.clone();
                                    tokio::spawn(async move {
                                        let quota = fetch_quota_from_api(&auth).await;
                                        let _ = tx.send((profile_id, quota)).await;
                                    });
                                }
                            }
                        }
                        None => {}
                    }
                }
            }
            Some(input::Action::Tick) | None => {}
        }
    }
    Ok(())
}

async fn poll_quota_loop<F, Fut>(
    profiles_dir: std::path::PathBuf,
    tx: tokio::sync::mpsc::Sender<(String, Option<nexus::codex_profiles::quota::QuotaSnapshot>)>,
    fetch_profile: F,
) where
    F: Fn(String, std::path::PathBuf) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = (String, Option<nexus::codex_profiles::quota::QuotaSnapshot>)>
        + Send
        + 'static,
{
    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(30));

    loop {
        interval.tick().await;
        refresh_all_profiles_once(&profiles_dir, &tx, &fetch_profile).await;
    }
}

async fn refresh_all_profiles_once<F, Fut>(
    profiles_dir: &std::path::Path,
    tx: &tokio::sync::mpsc::Sender<(String, Option<nexus::codex_profiles::quota::QuotaSnapshot>)>,
    fetch_profile: &F,
) where
    F: Fn(String, std::path::PathBuf) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = (String, Option<nexus::codex_profiles::quota::QuotaSnapshot>)>
        + Send
        + 'static,
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

    while let Some(Ok((profile_id, quota))) = set.join_next().await {
        let _ = tx.send((profile_id, quota)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::poll_quota_loop;
    use chrono::Utc;
    use nexus::codex_profiles::quota::{Confidence, LimitState, QuotaSnapshot};
    use std::fs;
    use std::time::Duration;
    use tempfile::tempdir;
    use tokio::sync::mpsc;

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
        let handle = tokio::spawn(poll_quota_loop(temp.path().to_path_buf(), tx, |pid, _| async move {
            (pid, Some(test_snapshot()))
        }));

        let (profile_id, quota) = tokio::time::timeout(Duration::from_millis(100), rx.recv())
            .await
            .expect("quota refresh should happen immediately")
            .expect("quota message");

        assert_eq!(profile_id, "profile-01");
        assert!(quota.is_some(), "expected startup refresh result");

        handle.abort();
    }

    #[tokio::test]
    async fn poll_quota_loop_forwards_failed_refreshes() {
        let temp = tempdir().expect("tempdir");
        let profile_dir = temp.path().join("profile-01");
        fs::create_dir_all(&profile_dir).expect("profile dir");

        let (tx, mut rx) = mpsc::channel(4);
        let handle = tokio::spawn(poll_quota_loop(temp.path().to_path_buf(), tx, |pid, _| async move {
            (pid, None)
        }));

        let (profile_id, quota) = tokio::time::timeout(Duration::from_millis(100), rx.recv())
            .await
            .expect("failed background refresh should still be forwarded")
            .expect("quota message");

        assert_eq!(profile_id, "profile-01");
        assert!(quota.is_none(), "expected failed refresh to be forwarded");

        handle.abort();
    }
}
