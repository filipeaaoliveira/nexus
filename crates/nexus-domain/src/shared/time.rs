use chrono::{DateTime, Local};

/// Convert epoch milliseconds to local DateTime. Returns None for 0 or invalid values.
pub fn from_epoch_millis(ms: i64) -> Option<DateTime<Local>> {
    if ms == 0 {
        return None;
    }
    chrono::DateTime::from_timestamp_millis(ms).map(|dt| dt.with_timezone(&Local))
}

/// Convert epoch seconds to local DateTime. Returns None for 0 or invalid values.
pub fn from_epoch_secs(secs: i64) -> Option<DateTime<Local>> {
    if secs == 0 {
        return None;
    }
    chrono::DateTime::from_timestamp(secs, 0).map(|dt| dt.with_timezone(&Local))
}

/// Parse an RFC3339 timestamp string to local DateTime. Returns None for invalid input.
pub fn from_rfc3339(s: &str) -> Option<DateTime<Local>> {
    chrono::DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|dt| dt.with_timezone(&Local))
}
