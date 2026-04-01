use std::path::PathBuf;

use clap::Parser;

#[derive(Parser, Debug)]
#[command(
    name = "ai-overview",
    about = "Read-only terminal dashboard for monitoring AI coding agents"
)]
pub struct Cli {
    /// Poll interval in seconds
    #[arg(long, default_value = "2")]
    pub poll_interval: u64,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub poll_interval_secs: u64,
    pub worktree_scan_interval_secs: u64,
    pub opencode_api_url: Option<String>,
    pub claude_data_dir: PathBuf,
    pub codex_data_dir: PathBuf,
    pub opencode_data_dir: PathBuf,
}

impl Default for Config {
    fn default() -> Self {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
        Self {
            poll_interval_secs: 2,
            worktree_scan_interval_secs: 10,
            opencode_api_url: None,
            claude_data_dir: home.join(".claude"),
            codex_data_dir: home.join(".codex"),
            opencode_data_dir: home.join(".local/share/opencode"),
        }
    }
}

impl Config {
    pub fn from_cli(cli: &Cli) -> Self {
        Config {
            poll_interval_secs: cli.poll_interval,
            ..Config::default()
        }
    }
}
