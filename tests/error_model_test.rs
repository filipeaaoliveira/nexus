use std::fs;

use nexus::testing::{profile_io, ProfileMetadataError};
use tempfile::tempdir;

#[test]
fn invalid_profile_metadata_returns_an_error() {
    let tmp = tempdir().unwrap();
    fs::write(tmp.path().join("profile.json"), "{ not valid json").unwrap();

    let err = profile_io::load_profile_metadata(tmp.path())
        .expect_err("invalid metadata should not parse");
    assert!(err.to_string().contains("profile.json"));
}

#[test]
fn invalid_profile_metadata_is_reported_as_corruption_not_silently_hidden() {
    let tmp = tempdir().unwrap();
    fs::write(tmp.path().join("profile.json"), "{ not valid json").unwrap();

    let err = profile_io::load_profile_metadata(tmp.path())
        .expect_err("invalid metadata should be a typed corruption error");
    assert!(matches!(err, ProfileMetadataError::Corrupt { .. }));
    assert!(err.to_string().contains("corruption"));
}
