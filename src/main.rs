use ai_overview::app::AppState;
use ai_overview::config::{Cli, Config};
use ai_overview::detect;
use ai_overview::detect::worktree;
use ai_overview::event::AgentEvent;
use ai_overview::input;
use clap::Parser;
use std::io::{IsTerminal, stdout};
use std::time::Duration;
use tokio::sync::mpsc;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing — log to a file so debug output doesn't corrupt the TUI.
    // Use RUST_LOG env var to control level (default: warn).
    // Logs go to /tmp/ai-overview.log when enabled.
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("ai_overview=warn"));
    let log_file = std::fs::File::create("/tmp/ai-overview.log")?;
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(log_file)
        .with_ansi(false)
        .init();

    // Check that we're running in a real terminal (like Codex CLI does).
    if !std::io::stdin().is_terminal() {
        anyhow::bail!("stdin is not a terminal — ai-overview must run in an interactive terminal");
    }
    if !stdout().is_terminal() {
        anyhow::bail!("stdout is not a terminal — ai-overview must run in an interactive terminal");
    }

    let cli = Cli::parse();
    let config = Config::from_cli(&cli);
    let mut state = AppState::new(config.clone());

    let mut rx = detect::spawn_detectors(&config);

    let mut terminal = ratatui::init();
    terminal.clear()?;

    let result = run_loop(&mut terminal, &mut state, &mut rx).await;

    ratatui::restore();
    result
}

async fn run_loop(
    terminal: &mut ratatui::DefaultTerminal,
    state: &mut AppState,
    rx: &mut mpsc::Receiver<AgentEvent>,
) -> anyhow::Result<()> {
    loop {
        // Drain all pending events
        while let Ok(event) = rx.try_recv() {
            if let AgentEvent::Discovered {
                tool,
                pid,
                ref cwd,
                ..
            } = event
            {
                let cwd_clone = cwd.clone();
                state.handle_event(event);
                let (branch, is_wt) = worktree::lookup_branch_info(&cwd_clone).await;
                state.handle_event(AgentEvent::SessionInfo {
                    pid,
                    tool,
                    session_id: String::new(),
                    branch,
                    is_worktree: is_wt,
                    started_at: None,
                });
                continue;
            }
            state.handle_event(event);
        }

        terminal.draw(|frame| {
            ai_overview::ui::render(frame, state);
        })?;

        match input::poll_action(Duration::from_millis(100), state.filter_mode)? {
            Some(input::Action::Quit) => break,
            Some(input::Action::MoveDown) => state.move_selection_down(),
            Some(input::Action::MoveUp) => state.move_selection_up(),
            Some(input::Action::CycleSortMode) => state.cycle_sort_mode(),
            Some(input::Action::ToggleGroupByWorktree) => state.toggle_group_by_worktree(),
            Some(input::Action::ShowHelp) => state.toggle_help(),
            Some(input::Action::RemoveAgent) => state.remove_selected(),
            Some(input::Action::RemoveExited) => state.remove_exited(),
            Some(input::Action::DismissOverlay) => state.dismiss_overlay(),
            Some(input::Action::StartFilter) => state.start_filter(),
            Some(input::Action::FilterChar(c)) => state.filter_push_char(c),
            Some(input::Action::FilterBackspace) => state.filter_pop_char(),
            Some(input::Action::FilterConfirm) => state.confirm_filter(),
            Some(input::Action::Tick) | None => {}
        }
    }
    Ok(())
}
