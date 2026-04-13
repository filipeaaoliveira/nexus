mod bootstrap;
mod input;
pub mod logging;
mod quota;
mod session;
pub mod terminal;

use crate::app::{AppState, EffectResult, RefreshOrigin};
use crate::config::Config;
use crate::detect;
use crate::event::AgentEvent;

pub(crate) use session::RuntimeSession;

pub async fn run(config: Config) -> anyhow::Result<()> {
    logging::initialize_tracing(&config)?;
    terminal::ensure_interactive_terminal()?;
    terminal::install_panic_restore_hook();

    let mut rx = detect::spawn_detectors(&config);
    let root = bootstrap::load_root_state(&config);
    let app = AppState::from_root(root);
    let mut session = RuntimeSession::new(config, app);
    session.initialize_accounts().await;

    let mut terminal = terminal::init_terminal()?;
    let result = run_loop(&mut terminal, &mut session, &mut rx).await;

    ratatui::restore();
    result
}

async fn run_loop(
    terminal: &mut ratatui::DefaultTerminal,
    session: &mut RuntimeSession,
    rx: &mut tokio::sync::mpsc::Receiver<AgentEvent>,
) -> anyhow::Result<()> {
    let (effect_tx, mut effect_rx) = tokio::sync::mpsc::channel::<EffectResult>(16);

    let config = session.config().clone();

    tokio::spawn(quota::poll_quota_loop(
        config.codex_profiles_dir.clone(),
        effect_tx.clone(),
        {
            let config = config.clone();
            move |pid, _profile_dir| {
                let config = config.clone();
                async move { quota::refresh_codex_quota(&config, pid, RefreshOrigin::Background).await }
            }
        },
    ));

    tokio::spawn(quota::poll_quota_loop(
        config.claude_profiles_dir.clone(),
        effect_tx.clone(),
        {
            let config = config.clone();
            move |pid, _profile_dir| {
                let config = config.clone();
                async move { quota::refresh_claude_quota(&config, pid, RefreshOrigin::Background).await }
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
            session.handle_event(event);
        }

        while let Ok(result) = effect_rx.try_recv() {
            session.apply_effect_result(result);
        }

        session.clear_expired_accounts_status();

        terminal.draw(|frame| {
            crate::ui::render(frame, session);
        })?;

        let context = input::input_context(session);
        match crate::input::poll_action(std::time::Duration::from_millis(100), &context)? {
            Some(crate::input::Action::App(action)) => {
                if !input::handle_semantic_input_action(session, *action).await? {
                    break;
                }
            }
            Some(crate::input::Action::Runtime(action)) => {
                if !input::handle_runtime_input_action(session, &effect_tx, action).await? {
                    break;
                }
            }
            None => {}
        }
    }
    Ok(())
}
