mod bootstrap;
mod input;
pub mod logging;
mod quota;
mod session;
pub mod terminal;

use std::sync::Arc;

use futures_util::StreamExt;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::app::{AppState, EffectResult, RefreshOrigin};
use crate::config::Config;
use crate::detect;
use crate::event::AgentEvent;
use crate::services::accounts;

pub(crate) use session::RuntimeSession;

/// Owns the handles of all background tasks spawned by the runtime.
///
/// When this struct is dropped, the handles are aborted as a safety net.
/// The primary shutdown path is token cancellation, which causes tasks to
/// exit gracefully before this destructor runs.
struct RuntimeTasks {
    quota_codex: JoinHandle<()>,
    quota_claude: JoinHandle<()>,
    #[cfg(unix)]
    sigterm: JoinHandle<()>,
}

impl Drop for RuntimeTasks {
    fn drop(&mut self) {
        self.quota_codex.abort();
        self.quota_claude.abort();
        #[cfg(unix)]
        self.sigterm.abort();
    }
}

pub async fn run(config: Config) -> anyhow::Result<()> {
    logging::initialize_tracing(&config)?;
    terminal::ensure_interactive_terminal()?;
    terminal::install_panic_restore_hook();

    let (_detectors, mut rx) = detect::spawn_detectors(&config);
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

    let token = CancellationToken::new();
    let config = session.config_arc();

    let stores = accounts::provider::ConfigStoreProvider::from_config(&config);

    let quota_codex = tokio::spawn(quota::poll_quota_loop(
        config.codex_profiles_dir.clone(),
        effect_tx.clone(),
        {
            let config = Arc::clone(&config);
            let stores = stores.clone();
            move |pid, _profile_dir| {
                let config = Arc::clone(&config);
                let stores = stores.clone();
                async move {
                    quota::refresh_quota(
                        &config,
                        &stores,
                        nexus_domain::accounts::Provider::Codex,
                        pid,
                        RefreshOrigin::Background,
                    )
                    .await
                }
            }
        },
        token.child_token(),
    ));

    let quota_claude = tokio::spawn(quota::poll_quota_loop(
        config.claude_profiles_dir.clone(),
        effect_tx.clone(),
        {
            let config = Arc::clone(&config);
            let stores = stores.clone();
            move |pid, _profile_dir| {
                let config = Arc::clone(&config);
                let stores = stores.clone();
                async move {
                    quota::refresh_quota(
                        &config,
                        &stores,
                        nexus_domain::accounts::Provider::Claude,
                        pid,
                        RefreshOrigin::Background,
                    )
                    .await
                }
            }
        },
        token.child_token(),
    ));

    #[cfg(unix)]
    let sigterm = {
        let token = token.clone();
        tokio::spawn(async move {
            use tokio::signal::unix::{signal, SignalKind};
            if let Ok(mut stream) = signal(SignalKind::terminate()) {
                stream.recv().await;
                token.cancel();
            }
        })
    };

    let _tasks = RuntimeTasks {
        quota_codex,
        quota_claude,
        #[cfg(unix)]
        sigterm,
    };

    let mut event_stream = crossterm::event::EventStream::new();

    loop {
        // Draw first, then wait for the next event.
        session.clear_expired_accounts_status();
        terminal.draw(|frame| {
            crate::ui::render(frame, session);
        })?;

        tokio::select! {
            biased;

            _ = token.cancelled() => break,

            Some(event) = rx.recv() => {
                let now = chrono::Utc::now();
                session.dispatch_action(crate::app::Action::AgentEvent {
                    event: Box::new(event),
                    now,
                }).await;
                // Drain queued agent events before redrawing
                while let Ok(event) = rx.try_recv() {
                    let now = chrono::Utc::now();
                    session.dispatch_action(crate::app::Action::AgentEvent {
                        event: Box::new(event),
                        now,
                    }).await;
                }
            }

            Some(result) = effect_rx.recv() => {
                session.apply_effect_result(result);
                // Drain queued effect results before redrawing
                while let Ok(result) = effect_rx.try_recv() {
                    session.apply_effect_result(result);
                }
            }

            maybe_event = event_stream.next() => {
                match maybe_event {
                    Some(Ok(crossterm::event::Event::Key(key))) => {
                        let context = input::input_context(session);
                        if let Some(action) = crate::input::keymap::action_for_key(key, &context) {
                            match action {
                                crate::input::Action::App(action) => {
                                    if !input::handle_semantic_input_action(session, *action).await? {
                                        token.cancel();
                                        break;
                                    }
                                }
                                crate::input::Action::Runtime(action) => {
                                    if !input::handle_runtime_input_action(session, &effect_tx, action).await? {
                                        token.cancel();
                                        break;
                                    }
                                }
                            }
                        }
                    }
                    Some(Ok(crossterm::event::Event::Resize(_, _))) => {
                        // Redraw on next iteration
                    }
                    Some(Ok(_)) => {}
                    Some(Err(e)) => return Err(e.into()),
                    None => break,
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Simulates the SIGTERM handler pattern: a mock signal fires, the
    /// cancellation token is cancelled, and child tasks observe the
    /// cancellation and wind down promptly.
    #[cfg(unix)]
    #[tokio::test]
    async fn sigterm_cancels_token_and_child_tasks_wind_down() {
        let token = CancellationToken::new();
        let signal = std::sync::Arc::new(tokio::sync::Notify::new());

        // Spawn a task that mimics the SIGTERM handler: wait for a mock
        // signal, then cancel the token.
        let handler = tokio::spawn({
            let token = token.clone();
            let signal = signal.clone();
            async move {
                signal.notified().await;
                token.cancel();
            }
        });

        // Spawn a child task that should exit when the token is cancelled.
        let child = tokio::spawn({
            let child_token = token.child_token();
            async move {
                child_token.cancelled().await;
                "child exited"
            }
        });

        // Token should still be live before the signal fires.
        assert!(!token.is_cancelled());

        // Fire the mock signal.
        signal.notify_one();

        // Both tasks should complete promptly.
        let timeout = std::time::Duration::from_secs(2);
        tokio::time::timeout(timeout, handler)
            .await
            .expect("handler did not finish in time")
            .expect("handler panicked");

        assert!(token.is_cancelled());

        let child_result = tokio::time::timeout(timeout, child)
            .await
            .expect("child did not finish in time")
            .expect("child panicked");
        assert_eq!(child_result, "child exited");
    }
}
