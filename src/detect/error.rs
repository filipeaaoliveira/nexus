use thiserror::Error;

#[derive(Debug, Error)]
pub enum DetectorError {
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
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
}
