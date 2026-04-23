use nexus::testing::{ProfileStoreError, StoreResult};

#[test]
fn profile_store_error_not_found_displays_profile_id() {
    let err = ProfileStoreError::NotFound {
        profile_id: "my-profile".to_string(),
    };
    assert!(err.to_string().contains("my-profile"));
    assert!(err.to_string().contains("not found"));
}

#[test]
fn profile_store_error_io_wraps_std_io_error() {
    let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file gone");
    let err = ProfileStoreError::Io(io_err);
    assert!(err.to_string().contains("file gone"));
}

#[test]
fn profile_store_error_other_preserves_context_and_source() {
    let err = ProfileStoreError::other("loading profile", "sqlite busy");
    let msg = err.to_string();
    assert!(msg.contains("loading profile"));
    assert!(msg.contains("sqlite busy"));
}

#[test]
fn store_result_err_is_ergonomic() {
    fn failing_op() -> StoreResult<()> {
        Err(ProfileStoreError::NotFound {
            profile_id: "test".to_string(),
        })
    }
    let result = failing_op();
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("test"));
}
