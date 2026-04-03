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

    /// Path to the log file used when RUST_LOG enables file logging
    #[arg(long)]
    pub log_file: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub poll_interval_secs: u64,
    pub worktree_scan_interval_secs: u64,
    pub log_file: PathBuf,
    pub opencode_api_url: Option<String>,
    pub claude_data_dir: PathBuf,
    pub codex_data_dir: PathBuf,
    pub opencode_data_dir: PathBuf,
}

impl Default for Config {
    fn default() -> Self {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
        let cache_dir = dirs::cache_dir().unwrap_or_else(|| home.join(".cache"));
        Self {
            poll_interval_secs: 2,
            worktree_scan_interval_secs: 10,
            log_file: cache_dir.join("ai-overview").join("ai-overview.log"),
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
            log_file: cli
                .log_file
                .clone()
                .unwrap_or_else(|| Config::default().log_file),
            ..Config::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Cli, Config};
    use clap::Parser;
    use std::path::PathBuf;

    #[test]
    fn test_from_cli_uses_explicit_log_file() {
        let cli = Cli::parse_from(["ai-overview", "--log-file", "/tmp/custom.log"]);
        let config = Config::from_cli(&cli);
        assert_eq!(config.log_file, PathBuf::from("/tmp/custom.log"));
    }

    #[test]
    fn test_default_log_file_uses_app_specific_name() {
        let config = Config::default();
        assert_eq!(
            config.log_file.file_name().and_then(|name| name.to_str()),
            Some("ai-overview.log")
        );
        assert!(config.log_file.to_string_lossy().contains("ai-overview"));
    }
}
