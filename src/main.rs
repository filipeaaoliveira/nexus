use clap::Parser;
use nexus::{run, Cli, Config};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let config = Config::from_cli(&cli);
    run(config).await
}
