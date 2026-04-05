use nexus::app::ActiveTab;
use nexus::app::AppState;
use nexus::config::{Cli, Config};
use nexus::detect;
use nexus::event::AgentEvent;
use nexus::input;
use nexus::codex_profiles::quota::fetch_quota_from_api;
use nexus::codex_profiles::ProfileAuth;
use clap::Parser;
use futures::future::join_all;
use std::io::{stdout, IsTerminal};
use std::time::Duration;
use tracing_subscriber::EnvFilter;

async fn fetch_quota_for_profile(profile_id: String, auth: ProfileAuth) -> (String, Option<nexus::codex_profiles::quota::QuotaSnapshot>) {
    let result = fetch_quota_from_api(&auth).await;
    (profile_id, result)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let config = Config::from_cli(&cli);
    initialize_tracing(&config)?;

    // Let clap handle non-interactive help/version output before enforcing TUI startup.
    if !std::io::stdin().is_terminal() {
        anyhow::bail!("stdin is not a terminal — ai-overview must run in an interactive terminal");
    }
    if !stdout().is_terminal() {
        anyhow::bail!("stdout is not a terminal — ai-overview must run in an interactive terminal");
    }

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
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("ai_overview=info"));
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

async fn run_loop(
    terminal: &mut ratatui::DefaultTerminal,
    state: &mut AppState,
    rx: &mut tokio::sync::mpsc::Receiver<AgentEvent>,
) -> anyhow::Result<()> {
    // Channel for async quota fetch results
    let (quota_tx, mut quota_rx) = tokio::sync::mpsc::channel::<(String, nexus::codex_profiles::quota::QuotaSnapshot)>(10);
    
    // Spawn background quota fetcher
    let config = state.config.clone();
    let profiles_dir = config.codex_profiles_dir.clone();
    
    tokio::spawn(async move {
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
                    Some(fetch_quota_for_profile(profile_id.clone(), auth))
                })
                .collect();
            
            let results = join_all(fetch_futures).await;
            
            for (profile_id, quota) in results {
                if let Some(quota) = quota {
                    let _ = quota_tx.send((profile_id, quota)).await;
                }
            }
        }
    });
    
    loop {
        // Drain all pending events
        while let Ok(event) = rx.try_recv() {
            state.handle_event(event);
        }

        // Check for async quota results (non-blocking)
        while let Ok((profile_id, quota)) = quota_rx.try_recv() {
            state.update_quota_for_profile(&profile_id, quota);
        }

        terminal.draw(|frame| {
            nexus::ui::render(frame, state);
        })?;

        match input::poll_action(
            Duration::from_millis(100),
            state.active_filter_mode(),
            state.active_rename_mode(),
            state.codex_tab.session_picker_mode,
            state.codex_tab.edit_mode,
            state.codex_tab.quota_edit_mode,
        )? {
            Some(input::Action::Quit) => break,
            Some(input::Action::MoveDown) => match state.active_tab() {
                ActiveTab::Overview => state.move_selection_down(),
                ActiveTab::Codex => state.handle_codex_move_down(),
            },
            Some(input::Action::MoveUp) => match state.active_tab() {
                ActiveTab::Overview => state.move_selection_up(),
                ActiveTab::Codex => state.handle_codex_move_up(),
            },
            Some(input::Action::NextTab) => state.next_tab(),
            Some(input::Action::CycleSortMode) => state.cycle_sort_mode(),
            Some(input::Action::ShowHelp) => state.toggle_help(),
            Some(input::Action::RemoveAgent) => match state.active_tab() {
                ActiveTab::Overview => state.remove_selected(),
                ActiveTab::Codex => {
                    if let Err(e) = state.delete_codex_profile() {
                        state.codex_tab.status_message = Some(e.to_string());
                    }
                }
            },
            Some(input::Action::RemoveExited) => match state.active_tab() {
                ActiveTab::Overview => state.remove_exited(),
                ActiveTab::Codex => {}
            },
            Some(input::Action::DismissOverlay) => match state.active_tab() {
                ActiveTab::Overview => state.dismiss_overlay(),
                ActiveTab::Codex => state.dismiss_codex_overlay(),
            },
            Some(input::Action::StartFilter) => match state.active_tab() {
                ActiveTab::Overview => state.start_filter(),
                ActiveTab::Codex => state.start_codex_filter(),
            },
            Some(input::Action::FilterChar(c)) => match state.active_tab() {
                ActiveTab::Overview => state.filter_push_char(c),
                ActiveTab::Codex => state.codex_filter_push_char(c),
            },
            Some(input::Action::FilterBackspace) => match state.active_tab() {
                ActiveTab::Overview => state.filter_pop_char(),
                ActiveTab::Codex => state.codex_filter_pop_char(),
            },
            Some(input::Action::FilterConfirm) => match state.active_tab() {
                ActiveTab::Overview => state.confirm_filter(),
                ActiveTab::Codex => state.confirm_codex_filter(),
            },
            Some(input::Action::StartRename) => match state.active_tab() {
                ActiveTab::Overview => state.start_rename(),
                ActiveTab::Codex => {}
            },
            Some(input::Action::CodexImportAuth) => {
                if state.active_tab() == ActiveTab::Codex {
                    if let Err(error) = state.import_codex_auth() {
                        state.codex_tab.status_message = Some(error.to_string());
                    }
                }
            }
            Some(input::Action::CodexWriteToHome) => {
                if state.active_tab() == ActiveTab::Codex {
                    if let Err(e) = state.write_active_profile_to_codex_home() {
                        state.codex_tab.status_message = Some(e.to_string());
                    }
                }
            }
            Some(input::Action::CodexLaunchSelected) => {
                // Enter does nothing - launch via terminal instead
            }
            Some(input::Action::RenameChar(c)) => match state.active_tab() {
                ActiveTab::Overview => state.rename_push_char(c),
                ActiveTab::Codex => {}
            },
            Some(input::Action::RenameBackspace) => match state.active_tab() {
                ActiveTab::Overview => state.rename_pop_char(),
                ActiveTab::Codex => {}
            },
            Some(input::Action::RenameConfirm) => match state.active_tab() {
                ActiveTab::Overview => state.confirm_rename(),
                ActiveTab::Codex => {}
            },
            Some(input::Action::CodexSessionPickerUp) => {
                if state.codex_tab.session_picker_mode {
                    state.session_picker_move_up();
                }
            }
            Some(input::Action::CodexSessionPickerDown) => {
                if state.codex_tab.session_picker_mode {
                    state.session_picker_move_down();
                }
            }
            Some(input::Action::CodexSessionPickerConfirm) => {
                if state.codex_tab.session_picker_mode {
                    state.confirm_session_picker();
                }
            }
            Some(input::Action::CodexSessionPickerCancel) => {
                if state.codex_tab.session_picker_mode {
                    state.dismiss_session_picker();
                }
            }
            Some(input::Action::CodexEditStart) => {
                if state.active_tab() == ActiveTab::Codex {
                    state.start_codex_edit();
                }
            }
            Some(input::Action::CodexEditChar(c)) => {
                if state.codex_tab.edit_mode {
                    state.codex_edit_push_char(c);
                }
            }
            Some(input::Action::CodexEditBackspace) => {
                if state.codex_tab.edit_mode {
                    state.codex_edit_pop_char();
                }
            }
            Some(input::Action::CodexEditToggleEnabled) => {
                if state.codex_tab.edit_mode {
                    state.codex_edit_toggle_enabled();
                }
            }
            Some(input::Action::CodexEditNextField) => {
                if state.codex_tab.edit_mode {
                    state.codex_edit_next_field();
                }
            }
            Some(input::Action::CodexEditPrevField) => {
                if state.codex_tab.edit_mode {
                    state.codex_edit_prev_field();
                }
            }
            Some(input::Action::CodexEditConfirm) => {
                if state.codex_tab.edit_mode {
                    if let Err(e) = state.confirm_codex_edit() {
                        state.codex_tab.status_message = Some(e.to_string());
                    }
                }
            }
            Some(input::Action::CodexEditCancel) => {
                if state.codex_tab.edit_mode {
                    state.dismiss_codex_edit();
                }
            }
            Some(input::Action::CodexQuotaEditStart) => {
                if state.active_tab() == ActiveTab::Codex {
                    state.start_codex_quota_edit();
                }
            }
            Some(input::Action::CodexQuotaEditChar(c)) => {
                if state.codex_tab.quota_edit_mode {
                    state.codex_quota_edit_push_char(c);
                }
            }
            Some(input::Action::CodexQuotaEditBackspace) => {
                if state.codex_tab.quota_edit_mode {
                    state.codex_quota_edit_pop_char();
                }
            }
            Some(input::Action::CodexQuotaEditNextField) => {
                if state.codex_tab.quota_edit_mode {
                    state.codex_quota_edit_next_field();
                }
            }
            Some(input::Action::CodexQuotaEditPrevField) => {
                if state.codex_tab.quota_edit_mode {
                    state.codex_quota_edit_prev_field();
                }
            }
            Some(input::Action::CodexQuotaEditConfirm) => {
                if state.codex_tab.quota_edit_mode {
                    if let Err(e) = state.confirm_codex_quota_edit() {
                        state.codex_tab.status_message = Some(e.to_string());
                    }
                }
            }
            Some(input::Action::CodexQuotaEditCancel) => {
                if state.codex_tab.quota_edit_mode {
                    state.dismiss_codex_quota_edit();
                }
            }
            Some(input::Action::CodexRefreshQuota) => {
                // Spawn async quota fetch in background - non-blocking
                let Some(profile_id) = state.selected_codex_profile_id().map(|s| s.to_string()) else {
                    continue;
                };
                let profile_dir = state.config.codex_profiles_dir.join(&profile_id);
                let auth_path = profile_dir.join("auth.json");
                
                if auth_path.exists() {
                    if let Ok(auth) = ProfileAuth::load(&auth_path) {
                        state.codex_tab.quota_loading = true;
                        
                        tokio::spawn(async move {
                            let _result = fetch_quota_from_api(&auth).await;
                            // Quota will be fetched - for now we just show it's loading
                        });
                    }
                }
            }
            Some(input::Action::Tick) | None => {}
        }
    }
    Ok(())
}
