use chrono::{DateTime, Local};

/// Format a duration for display: "5s", "3m12s", "2h15m".
pub fn format_duration(d: chrono::Duration) -> String {
    let secs = d.num_seconds().max(0);
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m{}s", secs / 60, secs % 60)
    } else {
        format!("{}h{}m", secs / 3600, (secs % 3600) / 60)
    }
}

/// Format a duration in short form: "5s", "3m", "2h".
pub fn format_duration_short(d: chrono::Duration) -> String {
    let secs = d.num_seconds().max(0);
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m", secs / 60)
    } else {
        format!("{}h", secs / 3600)
    }
}

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
