use std::io;
use std::path::PathBuf;
use std::time::Duration;

use chrono::Local;
use clap::Parser;
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use ai_overview::app::AppState;
use ai_overview::config::{Cli, Config};
use ai_overview::event::AgentEvent;
use ai_overview::input::{self, Action};
use ai_overview::types::*;

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let config = Config::from_cli(&cli);
    let mut state = AppState::new(config);

    // Inject fake agents for demonstration
    inject_fake_agents(&mut state);

    // Initialize terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Main loop
    let result = run_loop(&mut terminal, &mut state);

    // Restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    state: &mut AppState,
) -> anyhow::Result<()> {
    loop {
        terminal.draw(|frame| {
            ai_overview::ui::render(frame, state);
        })?;

        if state.should_quit {
            break;
        }

        match input::poll_action(Duration::from_millis(250))? {
            Some(Action::Quit) => {
                state.should_quit = true;
            }
            Some(Action::MoveDown) => state.move_selection_down(),
            Some(Action::MoveUp) => state.move_selection_up(),
            Some(Action::ToggleGroupByWorktree) => state.toggle_group_by_worktree(),
            Some(Action::CycleSortMode) => state.cycle_sort_mode(),
            Some(Action::ShowHelp) => state.toggle_help(),
            Some(Action::DismissOverlay) => state.dismiss_overlay(),
            Some(Action::StartFilter) => {
                // Filter input not yet implemented
            }
            Some(Action::Tick) | None => {}
        }
    }
    Ok(())
}

fn inject_fake_agents(state: &mut AppState) {
    let now = Local::now();

    // Agent 1: OpenCode, waiting, 2 subagents
    state.handle_event(AgentEvent::Discovered {
        tool: AgentTool::OpenCode,
        pid: 12001,
        cwd: PathBuf::from("/home/user/projects/webapp"),
    });
    state.handle_event(AgentEvent::StatusChanged {
        pid: 12001,
        tool: AgentTool::OpenCode,
        old: PrimaryStatus::Running,
        new: PrimaryStatus::Waiting,
    });
    state.handle_event(AgentEvent::SessionInfo {
        pid: 12001,
        tool: AgentTool::OpenCode,
        session_id: "sess-oc-001".to_string(),
        branch: Some("feat/auth-flow".to_string()),
        is_worktree: true,
    });
    state.handle_event(AgentEvent::SubagentSpawned {
        pid: 12001,
        tool: AgentTool::OpenCode,
        subagent: Subagent {
            name: "explore-codebase".to_string(),
            status: PrimaryStatus::Running,
            description: Some("searching for auth patterns".to_string()),
            started_at: now - chrono::Duration::seconds(45),
            ended_at: None,
        },
    });
    state.handle_event(AgentEvent::SubagentSpawned {
        pid: 12001,
        tool: AgentTool::OpenCode,
        subagent: Subagent {
            name: "run-tests".to_string(),
            status: PrimaryStatus::Done,
            description: Some("running test suite".to_string()),
            started_at: now - chrono::Duration::minutes(2),
            ended_at: Some(now - chrono::Duration::seconds(90)),
        },
    });

    // Backfill some started_at for realism
    if let Some(agent) = state.agents.iter_mut().find(|a| a.pid == 12001) {
        agent.started_at = now - chrono::Duration::minutes(8);
    }

    // Agent 2: Claude, running with tool call
    state.handle_event(AgentEvent::Discovered {
        tool: AgentTool::Claude,
        pid: 12002,
        cwd: PathBuf::from("/home/user/projects/api-server"),
    });
    state.handle_event(AgentEvent::SessionInfo {
        pid: 12002,
        tool: AgentTool::Claude,
        session_id: "sess-cl-042".to_string(),
        branch: Some("main".to_string()),
        is_worktree: false,
    });
    state.handle_event(AgentEvent::ToolCallStarted {
        pid: 12002,
        tool: AgentTool::Claude,
        tool_name: "Edit".to_string(),
        args_summary: Some("src/handlers/auth.rs".to_string()),
    });

    if let Some(agent) = state.agents.iter_mut().find(|a| a.pid == 12002) {
        agent.started_at = now - chrono::Duration::minutes(3);
    }

    // Agent 3: Codex, done
    state.handle_event(AgentEvent::Discovered {
        tool: AgentTool::Codex,
        pid: 12003,
        cwd: PathBuf::from("/home/user/projects/docs-site"),
    });
    state.handle_event(AgentEvent::StatusChanged {
        pid: 12003,
        tool: AgentTool::Codex,
        old: PrimaryStatus::Running,
        new: PrimaryStatus::Done,
    });
    state.handle_event(AgentEvent::SessionInfo {
        pid: 12003,
        tool: AgentTool::Codex,
        session_id: "sess-cx-007".to_string(),
        branch: Some("fix/typos".to_string()),
        is_worktree: false,
    });

    if let Some(agent) = state.agents.iter_mut().find(|a| a.pid == 12003) {
        agent.started_at = now - chrono::Duration::minutes(15);
    }

    // Agent 4: Claude, error
    state.handle_event(AgentEvent::Discovered {
        tool: AgentTool::Claude,
        pid: 12004,
        cwd: PathBuf::from("/home/user/projects/ml-pipeline"),
    });
    state.handle_event(AgentEvent::StatusChanged {
        pid: 12004,
        tool: AgentTool::Claude,
        old: PrimaryStatus::Running,
        new: PrimaryStatus::Error,
    });
    state.handle_event(AgentEvent::SessionInfo {
        pid: 12004,
        tool: AgentTool::Claude,
        session_id: "sess-cl-099".to_string(),
        branch: Some("feat/data-pipeline".to_string()),
        is_worktree: true,
    });

    if let Some(agent) = state.agents.iter_mut().find(|a| a.pid == 12004) {
        agent.started_at = now - chrono::Duration::minutes(1);
        // Add some fake events
        agent.recent_events.push_front(ActivityEvent {
            timestamp: now - chrono::Duration::seconds(30),
            kind: ActivityEventKind::ToolCall,
            description: "Bash: cargo build".to_string(),
        });
        agent.recent_events.push_front(ActivityEvent {
            timestamp: now - chrono::Duration::seconds(15),
            kind: ActivityEventKind::StatusChanged,
            description: "RUNNING → ERROR".to_string(),
        });
        agent.recent_events.push_front(ActivityEvent {
            timestamp: now - chrono::Duration::seconds(10),
            kind: ActivityEventKind::Message,
            description: "compilation failed: 3 errors".to_string(),
        });
    }
}
