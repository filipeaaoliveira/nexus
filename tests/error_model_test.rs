use std::fs;

use nexus::claude_profiles::profile::load_claude_profile_metadata;
use nexus::codex_profiles::profile::load_profile_metadata;
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
#[ignore = "forward-looking corruption policy for a later error-model task"]
fn invalid_profile_metadata_is_reported_as_corruption_not_silently_hidden() {
    let tmp = tempdir().unwrap();
    fs::write(tmp.path().join("profile.json"), "{ not valid json").unwrap();

    let err = load_profile_metadata(tmp.path())
        .expect_err("future typed corruption errors should surface invalid profile metadata");
    assert!(err.to_string().contains("corruption"));
}
