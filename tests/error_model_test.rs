use std::fs;

use nexus::testing::{load_claude_profile_metadata, load_profile_metadata};
use tempfile::tempdir;

#[test]
fn invalid_codex_profile_metadata_returns_an_error() {
    let tmp = tempdir().unwrap();
    fs::write(tmp.path().join("profile.json"), "{ not valid json").unwrap();

    let err = load_profile_metadata(tmp.path()).expect_err("invalid metadata should not parse");
    assert!(err.to_string().contains("profile.json"));
}

#[test]
fn invalid_claude_profile_metadata_returns_an_error() {
    let tmp = tempdir().unwrap();
    fs::write(tmp.path().join("profile.json"), "{ not valid json").unwrap();

    let err =
        load_claude_profile_metadata(tmp.path()).expect_err("invalid metadata should not parse");
    assert!(err.to_string().contains("profile.json"));
}

#[test]
#[ignore = "load_profile_metadata returns anyhow::Error — typed corruption errors require migrating profile loaders"]
fn invalid_profile_metadata_is_reported_as_corruption_not_silently_hidden() {
    let tmp = tempdir().unwrap();
    fs::write(tmp.path().join("profile.json"), "{ not valid json").unwrap();

    let err = load_profile_metadata(tmp.path())
        .expect_err("future typed corruption errors should surface invalid profile metadata");
    assert!(err.to_string().contains("corruption"));
}
