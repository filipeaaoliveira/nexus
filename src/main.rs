use clap::Parser;
use nexus::config::{Cli, Config};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let config = Config::from_cli(&cli);
    nexus::infra::config::migrate_legacy_dirs(&config);
    nexus::services::runtime::run(config).await
}
