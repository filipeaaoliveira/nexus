use ai_overview::app::AppState;
use ai_overview::config::{Cli, Config};
use ai_overview::detect;
use ai_overview::detect::worktree;
use ai_overview::event::AgentEvent;
use ai_overview::input;
use clap::Parser;
use crossterm::terminal::{self, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::ExecutableCommand;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use std::fs::File;
use std::time::Duration;
use tokio::sync::mpsc;

/// Open /dev/tty directly for the ratatui backend.
///
/// When running inside OpenCode (or similar agent wrappers), stdout is
/// redirected to a socket, so `ratatui::init()` (which uses stdout) fails
/// with "Device not configured" because crossterm cannot query the terminal
/// size on a non-TTY fd. Opening /dev/tty gives us a real terminal device.
fn init_terminal() -> anyhow::Result<Terminal<CrosstermBackend<File>>> {
    terminal::enable_raw_mode()?;
    let mut tty = File::options().read(true).write(true).open("/dev/tty")?;
    tty.execute(EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(tty);
    let terminal = Terminal::new(backend)?;
    Ok(terminal)
}

fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<File>>) {
    let _ = terminal::disable_raw_mode();
    let _ = terminal.backend_mut().execute(LeaveAlternateScreen);
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter("ai_overview=debug")
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();
    let config = Config::from_cli(&cli);
    let mut state = AppState::new(config.clone());

    let mut rx = detect::spawn_detectors(&config);

    let mut terminal = init_terminal()?;
    terminal.clear()?;

    let result = run_loop(&mut terminal, &mut state, &mut rx).await;

    restore_terminal(&mut terminal);
    result
}

async fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<File>>,
    state: &mut AppState,
    rx: &mut mpsc::Receiver<AgentEvent>,
) -> anyhow::Result<()> {
    loop {
        // Drain all pending events
        while let Ok(event) = rx.try_recv() {
            if let AgentEvent::Discovered { tool, pid, ref cwd } = event {
                let cwd_clone = cwd.clone();
                state.handle_event(event);
                let (branch, is_wt) = worktree::lookup_branch_info(&cwd_clone).await;
                state.handle_event(AgentEvent::SessionInfo {
                    pid,
                    tool,
                    session_id: String::new(),
                    branch,
                    is_worktree: is_wt,
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
