use std::io::{stdout, IsTerminal};

pub fn ensure_interactive_terminal() -> anyhow::Result<()> {
    if !std::io::stdin().is_terminal() {
        anyhow::bail!("stdin is not a terminal — nexus must run in an interactive terminal");
    }
    if !stdout().is_terminal() {
        anyhow::bail!("stdout is not a terminal — nexus must run in an interactive terminal");
    }
    Ok(())
}

pub fn install_panic_restore_hook() {
    let prev_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        ratatui::restore();
        prev_hook(info);
    }));
}

pub fn init_terminal() -> anyhow::Result<ratatui::DefaultTerminal> {
    let mut terminal = ratatui::init();
    terminal.clear()?;
    Ok(terminal)
}
