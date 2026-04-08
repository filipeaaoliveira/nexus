use std::path::PathBuf;

use clap::Parser;

#[derive(Parser, Debug)]
#[command(
    name = "nexus",
    about = "Read-only terminal dashboard for monitoring AI coding agents"
)]
pub struct Cli {
    /// Poll interval in seconds
    #[arg(long, default_value = "5")]
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
    pub codex_management_dir: PathBuf,
    pub codex_binary: Option<PathBuf>,
    pub codex_launch_args: Vec<String>,
    pub codex_profiles_dir: PathBuf,
    pub codex_repos_dir: PathBuf,
    pub codex_runtime_dir: PathBuf,
    pub codex_sessions_dir: PathBuf,
    pub codex_quota_cache_file: PathBuf,
    pub opencode_data_dir: PathBuf,
    pub claude_management_dir: PathBuf,
    pub claude_profiles_dir: PathBuf,
    pub claude_quota_cache_file: PathBuf,
}

impl Default for Config {
    fn default() -> Self {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
        let data_dir = dirs::data_dir().unwrap_or_else(|| home.join(".local/share"));
        let cache_dir = dirs::cache_dir().unwrap_or_else(|| home.join(".cache"));
        let codex_data_dir = home.join(".codex");
        let codex_management_dir = data_dir.join("nexus").join("codex");
        let claude_management_dir = data_dir.join("nexus").join("claude");
        let opencode_data_dir = home.join(".local/share/opencode");
        Self {
            poll_interval_secs: 5,
            worktree_scan_interval_secs: 10,
            log_file: cache_dir.join("nexus").join("nexus.log"),
            opencode_api_url: None,
            claude_data_dir: home.join(".claude"),
            codex_data_dir: codex_data_dir.clone(),
            codex_management_dir: codex_management_dir.clone(),
            codex_binary: None,
            codex_launch_args: Vec::new(),
            codex_profiles_dir: codex_management_dir.join("profiles"),
            codex_repos_dir: codex_management_dir.join("repos"),
            codex_runtime_dir: codex_management_dir.join("runtime"),
            codex_sessions_dir: codex_data_dir.join("sessions"),
            codex_quota_cache_file: codex_management_dir.join("quota-cache.json"),
            opencode_data_dir,
            claude_management_dir: claude_management_dir.clone(),
            claude_profiles_dir: claude_management_dir.join("profiles"),
            claude_quota_cache_file: claude_management_dir.join("quota-cache.json"),
        }
    }
}

impl Config {
    pub fn from_cli(cli: &Cli) -> Self {
        let defaults = Config::default();
        Config {
            poll_interval_secs: cli.poll_interval,
            log_file: cli.log_file.clone().unwrap_or(defaults.log_file),
            ..defaults
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
        let cli = Cli::parse_from(["nexus", "--log-file", "/tmp/custom.log"]);
        let config = Config::from_cli(&cli);
        assert_eq!(config.log_file, PathBuf::from("/tmp/custom.log"));
    }

    #[test]
    fn test_claude_profiles_dir_under_nexus() {
        let config = Config::default();
        let s = config.claude_profiles_dir.to_string_lossy();
        assert!(s.contains("nexus"), "claude_profiles_dir must be under nexus data dir");
        assert!(s.ends_with("profiles"), "claude_profiles_dir must end with profiles");
    }

    #[test]
    fn test_default_log_file_uses_app_specific_name() {
        let config = Config::default();
        assert_eq!(
            config.log_file.file_name().and_then(|name| name.to_str()),
            Some("nexus.log")
        );
        assert!(config.log_file.to_string_lossy().contains("nexus"));
    }
}
