use ai_overview::app::ActiveTab;
use ai_overview::app::AppState;
use ai_overview::config::{Cli, Config};
use ai_overview::detect;
use ai_overview::event::AgentEvent;
use ai_overview::input;
use clap::Parser;
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
    // Use RUST_LOG to control level (default: warn).
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("ai_overview=warn"));
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
    loop {
        // Drain all pending events
        while let Ok(event) = rx.try_recv() {
            state.handle_event(event);
        }

        terminal.draw(|frame| {
            ai_overview::ui::render(frame, state);
        })?;

        match input::poll_action(
            Duration::from_millis(100),
            state.active_filter_mode(),
            state.active_rename_mode(),
            state.codex_tab.session_picker_mode,
        )? {
            Some(input::Action::Quit) => break,
            Some(input::Action::MoveDown) => match state.active_tab() {
                ActiveTab::Overview => state.move_selection_down(),
                ActiveTab::Codex => {
                    let previous_active = state.active_codex_profile_id().map(String::from);
                    state.handle_codex_move_down();
                    let new_active = state.active_codex_profile_id().map(String::from);
                    if previous_active.as_deref() != new_active.as_deref() {
                        if let Err(e) = state.restart_running_codex_with_profile() {
                            state.codex_tab.status_message = Some(e.to_string());
                        }
                    }
                }
            },
            Some(input::Action::MoveUp) => match state.active_tab() {
                ActiveTab::Overview => state.move_selection_up(),
                ActiveTab::Codex => {
                    let previous_active = state.active_codex_profile_id().map(String::from);
                    state.handle_codex_move_up();
                    let new_active = state.active_codex_profile_id().map(String::from);
                    if previous_active.as_deref() != new_active.as_deref() {
                        if let Err(e) = state.restart_running_codex_with_profile() {
                            state.codex_tab.status_message = Some(e.to_string());
                        }
                    }
                }
            },
            Some(input::Action::NextTab) => state.next_tab(),
            Some(input::Action::PreviousTab) => state.previous_tab(),
            Some(input::Action::CycleSortMode) => state.cycle_sort_mode(),
            Some(input::Action::ShowHelp) => state.toggle_help(),
            Some(input::Action::RemoveAgent) => match state.active_tab() {
                ActiveTab::Overview => state.remove_selected(),
                ActiveTab::Codex => {}
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
                if state.active_tab() == ActiveTab::Codex {
                    ratatui::restore();
                    let launch_result = state.launch_selected_codex_profile();
                    *terminal = ratatui::init();
                    terminal.clear()?;
                    if let Err(error) = launch_result {
                        state.codex_tab.status_message = Some(error.to_string());
                    }
                }
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
            Some(input::Action::Tick) | None => {}
        }
    }
    Ok(())
}
