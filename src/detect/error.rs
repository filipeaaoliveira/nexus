use thiserror::Error;

#[derive(Debug, Error)]
pub enum DetectorError {
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

impl DetectorError {
    /// Returns `true` if this error is transient and the operation should be retried.
    ///
    /// Transient errors:
    /// - Database: `DatabaseBusy`, `DatabaseLocked`
    /// - IO: `Interrupted`, `WouldBlock`, `TimedOut`
    ///
    /// All other errors are considered permanent.
    pub fn is_transient(&self) -> bool {
        match self {
            DetectorError::Database(e) => matches!(
                e.sqlite_error_code(),
                Some(rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked)
            ),
            DetectorError::Io(e) => matches!(
                e.kind(),
                std::io::ErrorKind::Interrupted
                    | std::io::ErrorKind::WouldBlock
                    | std::io::ErrorKind::TimedOut
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detector_error_from_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "missing");
        let err = DetectorError::from(io_err);
        assert!(matches!(err, DetectorError::Io(_)));
    }

    #[test]
    fn io_not_found_is_permanent() {
        let err = DetectorError::from(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "missing",
        ));
        assert!(!err.is_transient());
    }

    #[test]
    fn io_interrupted_is_transient() {
        let err = DetectorError::from(std::io::Error::new(
            std::io::ErrorKind::Interrupted,
            "interrupted",
        ));
        assert!(err.is_transient());
    }

    #[test]
    fn io_would_block_is_transient() {
        let err = DetectorError::from(std::io::Error::new(
            std::io::ErrorKind::WouldBlock,
            "would block",
        ));
        assert!(err.is_transient());
    }

    #[test]
    fn io_permission_denied_is_permanent() {
        let err = DetectorError::from(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "permission denied",
        ));
        assert!(!err.is_transient());
    }
}
