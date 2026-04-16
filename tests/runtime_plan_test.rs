use nexus::testing::{canonical_repo_root, repo_runtime_key};
use std::fs;
use std::path::PathBuf;
use tempfile::tempdir;

#[test]
fn runtime_repo_key_is_repo_specific_and_not_current_repo() {
    let temp = tempdir().expect("tempdir");
    let repo_root = temp.path().join("project");

    fs::create_dir_all(&repo_root).expect("repo root");

    let key = repo_runtime_key(&repo_root);

    assert!(key.starts_with("project-"), "key was {key}");
    assert_ne!(key, "current-repo");
    assert!(key
        .chars()
        .all(|c: char| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'));
}

#[test]
fn canonical_repo_root_uses_git_toplevel_when_available() {
    let temp = tempdir().expect("tempdir");
    let repo_root = temp.path().join("workspace").join("project");
    let nested = repo_root.join("nested").join("deeper");

    fs::create_dir_all(&nested).expect("nested dir");

    let status = std::process::Command::new("git")
        .arg("init")
        .arg(&repo_root)
        .status()
        .expect("git init");
    assert!(status.success(), "git init failed with {status}");

    let resolved = canonical_repo_root(&nested).expect("repo root");

    assert_eq!(
        resolved,
        repo_root.canonicalize().expect("canonical repo root")
    );
}

#[test]
fn canonical_repo_root_falls_back_to_cwd_when_git_is_unavailable() {
    let cwd = PathBuf::from("/tmp/not-a-git-repo");

    let resolved = canonical_repo_root(&cwd).expect("repo root fallback");

    assert_eq!(resolved, cwd);
}
