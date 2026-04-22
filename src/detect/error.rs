use std::path::PathBuf;

use nexus_domain::system::DetectorKind;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DetectorError {
    #[error("{detector} database error at {}: {source}", path.display())]
    Database {
        detector: DetectorKind,
        path: PathBuf,
        source: rusqlite::Error,
    },

    #[error("{detector} I/O error ({context}): {source}")]
    Io {
        detector: DetectorKind,
        context: &'static str,
        source: std::io::Error,
    },
}

impl DetectorError {
    pub fn database(
        detector: DetectorKind,
        path: impl Into<PathBuf>,
        source: rusqlite::Error,
    ) -> Self {
        Self::Database {
            detector,
            path: path.into(),
            source,
        }
    }

    pub fn io(detector: DetectorKind, context: &'static str, source: std::io::Error) -> Self {
        Self::Io {
            detector,
            context,
            source,
        }
    }

    /// Returns `true` if this error is transient and the operation should be retried.
    pub fn is_transient(&self) -> bool {
        match self {
            DetectorError::Database { source, .. } => matches!(
                source.sqlite_error_code(),
                Some(rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked)
            ),
            DetectorError::Io { source, .. } => matches!(
                source.kind(),
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
    fn database_error_displays_context() {
        let err = DetectorError::database(
            DetectorKind::Codex,
            "/tmp/state.sqlite",
            rusqlite::Error::SqliteFailure(
                rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_BUSY),
                Some("database is locked".to_string()),
            ),
        );
        let msg = err.to_string();
        assert!(msg.contains("Codex"), "should contain detector name: {msg}");
        assert!(msg.contains("state.sqlite"), "should contain path: {msg}");
    }

    #[test]
    fn io_error_displays_context() {
        let err = DetectorError::io(
            DetectorKind::Claude,
            "reading transcript",
            std::io::Error::new(std::io::ErrorKind::NotFound, "missing"),
        );
        let msg = err.to_string();
        assert!(msg.contains("Claude"), "should contain detector name: {msg}");
        assert!(
            msg.contains("reading transcript"),
            "should contain context: {msg}"
        );
    }

    #[test]
    fn io_not_found_is_permanent() {
        let err = DetectorError::io(
            DetectorKind::Claude,
            "test",
            std::io::Error::new(std::io::ErrorKind::NotFound, "missing"),
        );
        assert!(!err.is_transient());
    }

    #[test]
    fn io_interrupted_is_transient() {
        let err = DetectorError::io(
            DetectorKind::Claude,
            "test",
            std::io::Error::new(std::io::ErrorKind::Interrupted, "interrupted"),
        );
        assert!(err.is_transient());
    }
}
