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
