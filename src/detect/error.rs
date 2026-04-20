use thiserror::Error;

#[derive(Debug, Error)]
pub enum DetectorError {
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("channel closed")]
    #[allow(dead_code)]
    ChannelClosed,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detector_error_display() {
        let err = DetectorError::ChannelClosed;
        assert_eq!(err.to_string(), "channel closed");
    }

    #[test]
    fn detector_error_from_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "missing");
        let err = DetectorError::from(io_err);
        assert!(matches!(err, DetectorError::Io(_)));
    }
}
