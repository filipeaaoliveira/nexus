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
            state.filter_mode,
            state.rename_mode,
        )? {
            Some(input::Action::Quit) => break,
            Some(input::Action::MoveDown) => state.move_selection_down(),
            Some(input::Action::MoveUp) => state.move_selection_up(),
            Some(input::Action::NextTab) => state.next_tab(),
            Some(input::Action::PreviousTab) => state.previous_tab(),
            Some(input::Action::CycleSortMode) => state.cycle_sort_mode(),
            Some(input::Action::ShowHelp) => state.toggle_help(),
            Some(input::Action::RemoveAgent) => state.remove_selected(),
            Some(input::Action::RemoveExited) => state.remove_exited(),
            Some(input::Action::DismissOverlay) => state.dismiss_overlay(),
            Some(input::Action::StartFilter) => state.start_filter(),
            Some(input::Action::FilterChar(c)) => state.filter_push_char(c),
            Some(input::Action::FilterBackspace) => state.filter_pop_char(),
            Some(input::Action::FilterConfirm) => state.confirm_filter(),
            Some(input::Action::StartRename) => state.start_rename(),
            Some(input::Action::RenameChar(c)) => state.rename_push_char(c),
            Some(input::Action::RenameBackspace) => state.rename_pop_char(),
            Some(input::Action::RenameConfirm) => state.confirm_rename(),
            Some(input::Action::Tick) | None => {}
        }
    }
    Ok(())
}
