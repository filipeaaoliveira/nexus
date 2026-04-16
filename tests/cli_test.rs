use nexus::config::Config;
use nexus::testing::{launch_codex, require_codex_binary, RuntimePlan};
use std::fs;
use std::path::PathBuf;
use tempfile::tempdir;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

#[test]
fn test_help_works_without_interactive_terminal() {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_nexus"))
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
        stdout.contains("Terminal dashboard for monitoring and managing AI coding agents"),
        "expected clap help output, stdout was:\n{stdout}"
    );
    assert!(
        stdout.contains("--poll-interval"),
        "expected poll interval flag in help output, stdout was:\n{stdout}"
    );
}

#[test]
fn default_codex_monitoring_uses_home_codex_dir() {
    let config = Config::default();
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"));

    assert_eq!(config.codex_data_dir, home.join(".codex"));
}

#[test]
fn default_codex_management_paths_are_under_nexus_data_root() {
    let config = Config::default();

    let management_root = config.codex_management_dir.clone();
    assert!(management_root.ends_with("nexus/codex"));
    assert_eq!(config.codex_profiles_dir, management_root.join("profiles"));
    assert_eq!(config.codex_repos_dir, management_root.join("repos"));
    assert_eq!(config.codex_runtime_dir, management_root.join("runtime"));
    assert_eq!(
        config.codex_quota_cache_file,
        management_root.join("quota-cache.json")
    );
}

#[test]
fn codex_runtime_plan_keeps_profile_auth_and_repo_sessions_separate() {
    let plan = RuntimePlan::new(
        PathBuf::from("/profiles/plus-a"),
        PathBuf::from("/repos/repo-1"),
        PathBuf::from("/runtime/run-1"),
    );

    assert_eq!(plan.auth_path, PathBuf::from("/profiles/plus-a/auth.json"));
    assert_eq!(plan.sessions_path, PathBuf::from("/repos/repo-1/sessions"));
    assert_eq!(plan.sqlite_home, PathBuf::from("/repos/repo-1/sqlite"));
}

#[test]
fn materialize_creates_runtime_auth_and_repo_session_layout() {
    let temp = tempdir().expect("tempdir");
    let profile_dir = temp.path().join("profiles/plus-a");
    let repo_dir = temp.path().join("repos/repo-1");
    let runtime_home = temp.path().join("runtime/run-1");

    fs::create_dir_all(&profile_dir).expect("profile dir");
    fs::create_dir_all(&repo_dir).expect("repo dir");
    fs::write(profile_dir.join("auth.json"), r#"{"token":"abc"}"#).expect("auth source");

    let plan = RuntimePlan::new(profile_dir, repo_dir.clone(), runtime_home.clone());
    plan.materialize().expect("materialize");

    assert_eq!(
        fs::read_to_string(runtime_home.join("auth.json")).expect("runtime auth"),
        r#"{"token":"abc"}"#
    );
    assert!(repo_dir.join("sessions").is_dir(), "sessions dir created");
    assert!(repo_dir.join("sqlite").is_dir(), "sqlite dir created");

    #[cfg(unix)]
    {
        let sessions_link = runtime_home.join("sessions");
        assert_eq!(
            fs::read_link(&sessions_link).expect("sessions link"),
            repo_dir.join("sessions")
        );
    }
}

#[cfg(unix)]
#[test]
fn require_codex_binary_rejects_non_executable_override() {
    let temp = tempdir().expect("tempdir");
    let binary = temp.path().join("codex");
    fs::write(&binary, "#!/bin/sh\necho codex\n").expect("binary");

    let mut perms = fs::metadata(&binary).expect("metadata").permissions();
    perms.set_mode(0o644);
    fs::set_permissions(&binary, perms).expect("permissions");
    assert!(require_codex_binary(Some(binary.as_path())).is_err());
}

#[cfg(unix)]
#[test]
fn launch_codex_places_injected_options_before_caller_args() {
    let temp = tempdir().expect("tempdir");
    let capture = temp.path().join("capture.txt");
    let script = temp.path().join("codex.sh");
    let runtime_home = temp.path().join("runtime");
    let profile_dir = temp.path().join("profiles/plus-a");
    let repo_dir = temp.path().join("repos/repo-1");
    let working_dir = temp.path().join("work");

    fs::create_dir_all(&profile_dir).expect("profile dir");
    fs::create_dir_all(&repo_dir).expect("repo dir");
    fs::create_dir_all(&working_dir).expect("work dir");
    fs::write(profile_dir.join("auth.json"), r#"{"token":"abc"}"#).expect("auth source");

    fs::write(
        &script,
        format!(
            "#!/bin/sh\nprintf 'cwd=%s\\n' \"$(pwd)\" > \"{}\"\nprintf 'code_home=%s\\n' \"$CODEX_HOME\" >> \"{}\"\nidx=0\nfor arg in \"$@\"; do\n  printf 'arg[%s]=%s\\n' \"$idx\" \"$arg\" >> \"{}\"\n  idx=$((idx + 1))\ndone\n",
            capture.display(),
            capture.display(),
            capture.display()
        ),
    )
    .expect("script");
    let mut perms = fs::metadata(&script).expect("metadata").permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&script, perms).expect("permissions");

    let plan = RuntimePlan::new(profile_dir, repo_dir.clone(), runtime_home.clone());
    let status = launch_codex(
        &script,
        &plan,
        &[
            "--".to_string(),
            "--user-flag".to_string(),
            "positional".to_string(),
        ],
        &working_dir,
    )
    .expect("launch");

    assert!(status.success(), "codex stub failed");

    let output = fs::read_to_string(&capture).expect("capture");
    let sqlite_home = repo_dir.join("sqlite").display().to_string();
    assert!(output.contains(&format!("code_home={}", runtime_home.display())));
    assert!(output.contains("arg[0]=-c"));
    assert!(output.contains(&format!("arg[1]=sqlite_home={sqlite_home}")));
    assert!(output.contains("arg[2]=--"));
    assert!(output.contains("arg[3]=--user-flag"));
    assert!(output.contains("arg[4]=positional"));
}
