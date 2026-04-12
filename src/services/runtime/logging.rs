use tracing_subscriber::EnvFilter;

use crate::config::Config;

pub fn initialize_tracing(config: &Config) -> anyhow::Result<()> {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("nexus=info"));
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
