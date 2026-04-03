use ai_overview::config::Config;

#[test]
fn test_help_works_without_interactive_terminal() {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_ai-overview"))
        .arg("--help")
        .output()
        .expect("failed to run ai-overview --help");

    assert!(
        output.status.success(),
        "expected --help to succeed, stderr was:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Read-only terminal dashboard for monitoring AI coding agents"),
        "expected clap help output, stdout was:\n{stdout}"
    );
    assert!(
        stdout.contains("--poll-interval"),
        "expected poll interval flag in help output, stdout was:\n{stdout}"
    );
}

#[test]
fn default_codex_management_paths_are_under_ai_overview_data_root() {
    let config = Config::default();

    let codex_root = config.codex_data_dir.clone();
    assert_eq!(config.codex_profiles_dir, codex_root.join("profiles"));
    assert_eq!(config.codex_repos_dir, codex_root.join("repos"));
    assert_eq!(config.codex_runtime_dir, codex_root.join("runtime"));
    assert_eq!(
        config.codex_quota_cache_file,
        codex_root.join("quota-cache.json")
    );
}
