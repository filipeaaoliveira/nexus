use clap::Parser;
use nexus::{migrate_legacy_dirs, run, Cli, Config};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let config = Config::from_cli(&cli);
    migrate_legacy_dirs(&config);
    run(config).await
}
