use nexus::app::ActiveTab;
use nexus::app::AppState;
use nexus::config::{Cli, Config};
use nexus::detect;
use nexus::event::AgentEvent;
use nexus::input;
use nexus::codex_profiles::quota::fetch_quota_from_api;
use nexus::codex_profiles::ProfileAuth;
use nexus::claude_profiles::quota::{fetch_claude_quota, refresh_token_if_needed};
use nexus::claude_profiles::auth::ClaudeAuth;
use clap::Parser;
use futures::future::join_all;
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

    migrate_data_dir_if_needed(&config);

    let mut rx = detect::spawn_detectors(&config);
    let mut state = AppState::new(config);

    let mut terminal = ratatui::init();
    terminal.clear()?;

    let result = run_loop(&mut terminal, &mut state, &mut rx).await;

    ratatui::restore();
    result
}

fn initialize_tracing(config: &Config) -> anyhow::Result<()> {
    // Initialize tracing to a file so debug output does not corrupt the TUI.
    // Use RUST_LOG to control level (default: info).
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("nexus=info"));
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
    let (quota_tx, mut quota_rx) = tokio::sync::mpsc::channel::<(String, Option<nexus::codex_profiles::quota::QuotaSnapshot>)>(10);

    // Spawn background quota fetcher
    let config = state.config.clone();
    let profiles_dir = config.codex_profiles_dir.clone();

    let bg_quota_tx = quota_tx.clone();
    tokio::spawn(async move {
        let quota_tx = bg_quota_tx;
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(30)).await;

            // Get list of profiles
            let profiles: Vec<String> = match std::fs::read_dir(&profiles_dir) {
                Ok(entries) => entries
                    .filter_map(|e| e.ok())
                    .filter(|e| e.path().is_dir())
                    .filter_map(|e| e.file_name().into_string().ok())
                    .collect(),
                Err(_) => continue,
            };

            // Fetch all quotas in parallel using join_all
            let fetch_futures: Vec<_> = profiles.iter()
                .filter_map(|profile_id| {
                    let auth_path = profiles_dir.join(profile_id).join("auth.json");
                    if !auth_path.exists() {
                        return None;
                    }
                    let auth = ProfileAuth::load(&auth_path).ok()?;
                    let pid = profile_id.clone();
                    Some(async move { (pid, fetch_quota_from_api(&auth).await) })
                })
                .collect();

            let results = join_all(fetch_futures).await;

            for (profile_id, quota) in results {
                if quota.is_some() {
                    let _ = quota_tx.send((profile_id, quota)).await;
                }
            }
        }
    });

    let (claude_quota_tx, mut claude_quota_rx) =
        tokio::sync::mpsc::channel::<(String, Option<nexus::codex_profiles::quota::QuotaSnapshot>)>(10);

    let claude_profiles_dir = config.claude_profiles_dir.clone();
    let bg_claude_tx = claude_quota_tx.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(30)).await;

            let profiles: Vec<String> = match std::fs::read_dir(&claude_profiles_dir) {
                Ok(entries) => entries
                    .filter_map(|e| e.ok())
                    .filter(|e| e.path().is_dir())
                    .filter_map(|e| e.file_name().into_string().ok())
                    .collect(),
                Err(_) => continue,
            };

            let fetch_futures: Vec<_> = profiles
                .iter()
                .filter_map(|profile_id| {
                    let creds_path = claude_profiles_dir.join(profile_id).join(".credentials.json");
                    if !creds_path.exists() {
                        return None;
                    }
                    let auth = ClaudeAuth::load(&creds_path).ok()?;
                    let pid = profile_id.clone();
                    Some(async move {
                        let refreshed = refresh_token_if_needed(auth).await;
                        if let Err(e) = refreshed.save(&creds_path) {
                            tracing::warn!("failed to persist refreshed Claude token: {}", e);
                        }
                        let quota = fetch_claude_quota(&refreshed).await;
                        (pid, quota)
                    })
                })
                .collect();

            let results = join_all(fetch_futures).await;
            for (profile_id, quota) in results {
                if quota.is_some() {
                    let _ = bg_claude_tx.send((profile_id, quota)).await;
                }
            }
        }
    });

    loop {
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

        terminal.draw(|frame| {
            nexus::ui::render(frame, state);
        })?;

        match input::poll_action(
            Duration::from_millis(100),
            state.active_filter_mode(),
            state.active_rename_mode(),
            state.accounts_tab.session_picker_mode,
            state.accounts_tab.edit_mode,
            state.accounts_tab.quota_edit_mode,
        )? {
            Some(input::Action::Quit) => break,
            Some(input::Action::MoveDown) => match state.active_tab() {
                ActiveTab::Overview => state.move_selection_down(),
                ActiveTab::Accounts => state.handle_codex_move_down(),
            },
            Some(input::Action::MoveUp) => match state.active_tab() {
                ActiveTab::Overview => state.move_selection_up(),
                ActiveTab::Accounts => state.handle_codex_move_up(),
            },
            Some(input::Action::NextTab) => state.next_tab(),
            Some(input::Action::CycleSortMode) => state.cycle_sort_mode(),
            Some(input::Action::ShowHelp) => state.toggle_help(),
            Some(input::Action::RemoveAgent) => match state.active_tab() {
                ActiveTab::Overview => state.remove_selected(),
                ActiveTab::Accounts => {
                    match state.selected_account() {
                        Some((nexus::app::Provider::Claude, _)) => {
                            if let Err(e) = state.delete_claude_profile() {
                                state.accounts_tab.status_message = Some(e.to_string());
                            }
                        }
                        Some((nexus::app::Provider::Codex, _)) | None => {
                            if let Err(e) = state.delete_codex_profile() {
                                state.accounts_tab.status_message = Some(e.to_string());
                            }
                        }
                    }
                }
            },
            Some(input::Action::RemoveExited) => match state.active_tab() {
                ActiveTab::Overview => state.remove_exited(),
                ActiveTab::Accounts => {}
            },
            Some(input::Action::DismissOverlay) => match state.active_tab() {
                ActiveTab::Overview => state.dismiss_overlay(),
                ActiveTab::Accounts => state.dismiss_codex_overlay(),
            },
            Some(input::Action::StartFilter) => match state.active_tab() {
                ActiveTab::Overview => state.start_filter(),
                ActiveTab::Accounts => state.start_codex_filter(),
            },
            Some(input::Action::FilterChar(c)) => match state.active_tab() {
                ActiveTab::Overview => state.filter_push_char(c),
                ActiveTab::Accounts => state.codex_filter_push_char(c),
            },
            Some(input::Action::FilterBackspace) => match state.active_tab() {
                ActiveTab::Overview => state.filter_pop_char(),
                ActiveTab::Accounts => state.codex_filter_pop_char(),
            },
            Some(input::Action::FilterConfirm) => match state.active_tab() {
                ActiveTab::Overview => state.confirm_filter(),
                ActiveTab::Accounts => state.confirm_codex_filter(),
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
                        state.accounts_tab.status_message = Some(e.to_string());
                    }
                }
            }
            Some(input::Action::AccountsWriteToHome) => {
                if state.active_tab() == ActiveTab::Accounts {
                    match state.selected_account() {
                        Some((nexus::app::Provider::Claude, _)) => {
                            if let Err(e) = state.write_selected_claude_profile_to_home() {
                                state.accounts_tab.status_message = Some(e.to_string());
                            }
                        }
                        Some((nexus::app::Provider::Codex, _)) | None => {
                            if let Err(e) = state.write_selected_profile_to_codex_home() {
                                state.accounts_tab.status_message = Some(e.to_string());
                            }
                        }
                    }
                }
            }
            Some(input::Action::CodexLaunchSelected) => {
                // Enter does nothing - launch via terminal instead
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
            Some(input::Action::CodexSessionPickerUp) => {
                if state.accounts_tab.session_picker_mode {
                    state.session_picker_move_up();
                }
            }
            Some(input::Action::CodexSessionPickerDown) => {
                if state.accounts_tab.session_picker_mode {
                    state.session_picker_move_down();
                }
            }
            Some(input::Action::CodexSessionPickerConfirm) => {
                if state.accounts_tab.session_picker_mode {
                    state.confirm_session_picker();
                }
            }
            Some(input::Action::CodexSessionPickerCancel) => {
                if state.accounts_tab.session_picker_mode {
                    state.dismiss_session_picker();
                }
            }
            Some(input::Action::CodexEditStart) => {
                if state.active_tab() == ActiveTab::Accounts {
                    state.start_codex_edit();
                }
            }
            Some(input::Action::CodexEditChar(c)) => {
                if state.accounts_tab.edit_mode {
                    state.codex_edit_push_char(c);
                }
            }
            Some(input::Action::CodexEditBackspace) => {
                if state.accounts_tab.edit_mode {
                    state.codex_edit_pop_char();
                }
            }
            Some(input::Action::CodexEditToggleEnabled) => {
                if state.accounts_tab.edit_mode {
                    state.codex_edit_toggle_enabled();
                }
            }
            Some(input::Action::CodexEditNextField) => {
                if state.accounts_tab.edit_mode {
                    state.codex_edit_next_field();
                }
            }
            Some(input::Action::CodexEditPrevField) => {
                if state.accounts_tab.edit_mode {
                    state.codex_edit_prev_field();
                }
            }
            Some(input::Action::CodexEditConfirm) => {
                if state.accounts_tab.edit_mode {
                    if let Err(e) = state.confirm_codex_edit() {
                        state.accounts_tab.status_message = Some(e.to_string());
                    }
                }
            }
            Some(input::Action::CodexEditCancel) => {
                if state.accounts_tab.edit_mode {
                    state.dismiss_codex_edit();
                }
            }
            Some(input::Action::CodexQuotaEditStart) => {
                if state.active_tab() == ActiveTab::Accounts {
                    state.start_codex_quota_edit();
                }
            }
            Some(input::Action::CodexQuotaEditChar(c)) => {
                if state.accounts_tab.quota_edit_mode {
                    state.codex_quota_edit_push_char(c);
                }
            }
            Some(input::Action::CodexQuotaEditBackspace) => {
                if state.accounts_tab.quota_edit_mode {
                    state.codex_quota_edit_pop_char();
                }
            }
            Some(input::Action::CodexQuotaEditNextField) => {
                if state.accounts_tab.quota_edit_mode {
                    state.codex_quota_edit_next_field();
                }
            }
            Some(input::Action::CodexQuotaEditPrevField) => {
                if state.accounts_tab.quota_edit_mode {
                    state.codex_quota_edit_prev_field();
                }
            }
            Some(input::Action::CodexQuotaEditConfirm) => {
                if state.accounts_tab.quota_edit_mode {
                    if let Err(e) = state.confirm_codex_quota_edit() {
                        state.accounts_tab.status_message = Some(e.to_string());
                    }
                }
            }
            Some(input::Action::CodexQuotaEditCancel) => {
                if state.accounts_tab.quota_edit_mode {
                    state.dismiss_codex_quota_edit();
                }
            }
            Some(input::Action::AccountsRefreshQuota) => {
                if state.active_tab() == ActiveTab::Accounts {
                    match state.selected_account() {
                        Some((nexus::app::Provider::Claude, profile_id)) => {
                            let profile_id = profile_id.to_string();
                            let creds_path = state.config.claude_profiles_dir.join(&profile_id).join(".credentials.json");
                            if creds_path.exists() {
                                if let Ok(auth) = ClaudeAuth::load(&creds_path) {
                                    state.accounts_tab.quota_loading = true;
                                    let tx = claude_quota_tx.clone();
                                    tokio::spawn(async move {
                                        let refreshed = refresh_token_if_needed(auth).await;
                                        if let Err(e) = refreshed.save(&creds_path) {
                                            tracing::warn!("failed to persist refreshed Claude token: {}", e);
                                        }
                                        let quota = fetch_claude_quota(&refreshed).await;
                                        let _ = tx.send((profile_id, quota)).await;
                                    });
                                }
                            }
                        }
                        Some((nexus::app::Provider::Codex, profile_id)) => {
                            let profile_id = profile_id.to_string();
                            let auth_path = state.config.codex_profiles_dir.join(&profile_id).join("auth.json");
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
            Some(input::Action::AccountsImportClaudeAuth) => {
                if state.active_tab() == ActiveTab::Accounts {
                    if let Err(e) = state.import_claude_auth() {
                        state.accounts_tab.status_message = Some(e.to_string());
                    }
                }
            }
            Some(input::Action::Tick) | None => {}
        }
    }
    Ok(())
}
