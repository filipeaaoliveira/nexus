use chrono::{DateTime, Utc};
use reqwest::{Client, StatusCode};
use serde::Deserialize;
use std::fmt;
use std::fs;
use std::path::Path;
use std::time::SystemTime;

use nexus_domain::accounts::quota::{
    Confidence, CreditsState, LimitState, QuotaSnapshot, QuotaWindow, WindowKind,
};

/// Deprecated compatibility wrapper for legacy `codex status` text parsing.
#[deprecated(note = "use parse_legacy_status_output for explicit legacy fallback parsing")]
#[allow(dead_code)]
pub fn parse_status_output(input: &str) -> Option<QuotaSnapshot> {
    parse_legacy_status_output(input)
}

/// Legacy text fallback for `codex status` output.
pub fn parse_legacy_status_output(input: &str) -> Option<QuotaSnapshot> {
    let (five_hour_state, five_hour_reset_at, five_hour_seen) = parse_window(input, "5h window");
    let (weekly_state, weekly_reset_at, weekly_seen) = parse_window(input, "weekly window");

    if !five_hour_seen && !weekly_seen {
        return None;
    }

    let summary = format!("5h={:?}; weekly={:?}", five_hour_state, weekly_state);

    Some(QuotaSnapshot::from_legacy(
        Utc::now(),
        Confidence::Parsed,
        five_hour_state,
        five_hour_reset_at,
        None,
        weekly_state,
        weekly_reset_at,
        None,
        summary,
    ))
}

fn parse_window(input: &str, prefix: &str) -> (LimitState, Option<DateTime<Utc>>, bool) {
    let Some(line) = input.lines().find(|line| line.contains(prefix)) else {
        return (LimitState::Unknown, None, false);
    };

    let reset_at = parse_reset_timestamp(line);
    let state = classify_window_state(line);

    (state, reset_at, true)
}

fn classify_window_state(line: &str) -> LimitState {
    let normalized = line.to_ascii_lowercase();

    if normalized.contains("limit reached")
        || normalized.contains("quota reached")
        || normalized.contains("hit the limit")
        || normalized.contains("usage limit")
        || normalized.contains("exhausted")
        || normalized.contains("unavailable")
    {
        return LimitState::Hit;
    }

    if normalized.contains("available") || normalized.contains("not reached") {
        return LimitState::Available;
    }

    LimitState::Unknown
}

fn parse_reset_timestamp(line: &str) -> Option<DateTime<Utc>> {
    let reset_marker = ["resets at ", "reset at "]
        .into_iter()
        .find_map(|marker| line.split_once(marker).map(|(_, rest)| rest))?;

    let candidate = reset_marker
        .trim()
        .trim_end_matches([',', '.', ';', ')'])
        .trim();

    DateTime::parse_from_rfc3339(candidate)
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
}

/// Scan `sessions_dir` (recursively) for the most-recently-modified `.jsonl` file,
/// find the last `token_count` event in it, and return a `QuotaSnapshot` built from
/// the `rate_limits` payload. Returns `None` if the directory is absent, empty, or
/// contains no `token_count` events.
pub fn parse_quota_from_sessions(sessions_dir: &Path) -> Option<QuotaSnapshot> {
    // Collect all .jsonl paths with their mtime.
    let mut files: Vec<(SystemTime, std::path::PathBuf)> = Vec::new();
    collect_jsonl_files(sessions_dir, &mut files);
    if files.is_empty() {
        return None;
    }

    // Sort newest first.
    files.sort_by(|a, b| b.0.cmp(&a.0));

    // Scan the newest file for the last token_count event.
    let content = fs::read_to_string(&files[0].1).ok()?;
    let last_event = content
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .rfind(|obj| {
            obj.get("type").and_then(|v| v.as_str()) == Some("event_msg")
                && obj.pointer("/payload/type").and_then(|v| v.as_str()) == Some("token_count")
        })?;

    let rate_limits = last_event.pointer("/payload/rate_limits")?;
    let windows = parse_windows_from_rate_limits(rate_limits);
    let credits = parse_credits_from_rate_limits(rate_limits);
    if windows.is_empty() && credits.is_none() {
        return None;
    }

    let summary = summarize_windows(&windows, credits.as_ref());

    Some(QuotaSnapshot::new(
        Utc::now(),
        Confidence::Parsed,
        windows,
        credits,
        summary,
    ))
}

fn pct_to_state(pct: Option<u8>) -> LimitState {
    match pct {
        Some(p) if p >= 100 => LimitState::Hit,
        Some(_) => LimitState::Available,
        None => LimitState::Unknown,
    }
}

fn collect_jsonl_files(dir: &Path, out: &mut Vec<(SystemTime, std::path::PathBuf)>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_jsonl_files(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("jsonl") {
            if let Ok(meta) = fs::metadata(&path) {
                if let Ok(mtime) = meta.modified() {
                    out.push((mtime, path));
                }
            }
        }
    }
}

fn parse_windows_from_rate_limits(rate_limits: &serde_json::Value) -> Vec<QuotaWindow> {
    if let Some(windows) = rate_limits
        .get("windows")
        .and_then(|value| value.as_array())
    {
        let mut parsed = Vec::new();
        for (index, window) in windows.iter().enumerate() {
            parsed.push(parse_window_value(window, index, None));
        }
        return parsed;
    }

    let mut parsed = Vec::new();
    if let Some(primary) = rate_limits
        .get("primary")
        .or_else(|| rate_limits.get("primary_window"))
    {
        parsed.push(parse_window_value(primary, 0, Some("5h")));
    }
    if let Some(secondary) = rate_limits
        .get("secondary")
        .or_else(|| rate_limits.get("secondary_window"))
    {
        parsed.push(parse_window_value(secondary, 1, Some("weekly")));
    }
    parsed
}

fn parse_window_value(
    window: &serde_json::Value,
    index: usize,
    fallback_label: Option<&str>,
) -> QuotaWindow {
    let label = window
        .get("label")
        .or_else(|| window.get("name"))
        .and_then(|value| value.as_str())
        .map(|value| value.to_string())
        .or_else(|| {
            window
                .get("windowDurationMins")
                .or_else(|| window.get("window_duration_mins"))
                .and_then(|value| value.as_i64())
                .map(label_from_window_minutes)
        })
        .or_else(|| fallback_label.map(str::to_string))
        .unwrap_or_else(|| format!("window-{}", index + 1));

    let used_pct = window
        .get("usedPercent")
        .or_else(|| window.get("used_percent"))
        .and_then(|value| value.as_f64())
        .map(|value| value.round().clamp(0.0, 100.0) as u8);

    let reset_at = window
        .get("resetsAt")
        .or_else(|| window.get("resets_at"))
        .or_else(|| window.get("reset_at"))
        .and_then(|value| {
            value
                .as_i64()
                .or_else(|| value.as_f64().map(|secs| secs as i64))
        })
        .and_then(|secs| DateTime::from_timestamp(secs, 0));

    let state = window
        .get("state")
        .or_else(|| window.get("limit_state"))
        .and_then(|value| value.as_str())
        .and_then(parse_limit_state)
        .unwrap_or_else(|| pct_to_state(used_pct));

    QuotaWindow {
        kind: WindowKind::from_label(&label),
        used_pct,
        reset_at,
        state,
    }
}

fn parse_credits_from_rate_limits(rate_limits: &serde_json::Value) -> Option<CreditsState> {
    let credits = rate_limits.get("credits")?;
    if credits.is_null() {
        return None;
    }

    let enabled = credits
        .get("enabled")
        .and_then(|value| value.as_bool())
        .unwrap_or(true);
    let balance_label = credits
        .get("balanceLabel")
        .or_else(|| credits.get("balance_label"))
        .and_then(|value| value.as_str())
        .map(|value| value.to_string());
    let unlimited = credits
        .get("unlimited")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);

    Some(CreditsState {
        enabled,
        balance_label,
        unlimited,
    })
}

fn parse_limit_state(value: &str) -> Option<LimitState> {
    match value.to_ascii_lowercase().as_str() {
        "hit" | "reached" | "exhausted" => Some(LimitState::Hit),
        "available" | "ok" | "open" => Some(LimitState::Available),
        "unknown" => Some(LimitState::Unknown),
        _ => None,
    }
}

fn label_from_window_minutes(minutes: i64) -> String {
    match minutes {
        300 => "5h".to_string(),
        10080 => "weekly".to_string(),
        1440 => "1d".to_string(),
        60 => "1h".to_string(),
        value if value % 60 == 0 => format!("{}h", value / 60),
        value => format!("{}m", value),
    }
}

fn summarize_windows(windows: &[QuotaWindow], credits: Option<&CreditsState>) -> String {
    let mut parts: Vec<String> = windows
        .iter()
        .map(|window| {
            let pct = window
                .used_pct
                .map(|pct| format!("{pct}%"))
                .unwrap_or_else(|| "?".to_string());
            format!("{}={}", window.kind.label(), pct)
        })
        .collect();

    if let Some(credits) = credits {
        if credits.enabled {
            let mut credit_summary = String::from("credits");
            if credits.unlimited {
                credit_summary.push_str("=unlimited");
            } else if let Some(balance_label) = credits.balance_label.as_ref() {
                credit_summary.push('=');
                credit_summary.push_str(balance_label);
            }
            parts.push(credit_summary);
        }
    }

    parts.join("; ")
}

use crate::codex_profiles::ProfileAuth;

#[derive(Debug, Deserialize)]
struct UsageResponse {
    #[serde(rename = "rate_limit")]
    rate_limit: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuotaApiError {
    Unauthorized,
    Other(String),
}

impl fmt::Display for QuotaApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unauthorized => write!(f, "HTTP 401 Unauthorized"),
            Self::Other(detail) => f.write_str(detail),
        }
    }
}

pub async fn fetch_quota_from_api(
    profile_auth: &ProfileAuth,
) -> Result<QuotaSnapshot, QuotaApiError> {
    let account_id = profile_auth
        .account_id()
        .ok_or_else(|| QuotaApiError::Other("missing account_id in auth".to_string()))?;
    let access_token = profile_auth
        .access_token()
        .ok_or_else(|| QuotaApiError::Other("missing access_token in auth".to_string()))?;
    fetch_quota_async(account_id, access_token).await
}

async fn fetch_quota_async(
    account_id: &str,
    access_token: &str,
) -> Result<QuotaSnapshot, QuotaApiError> {
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| QuotaApiError::Other(format!("client build failed: {e}")))?;

    let response = client
        .get("https://chatgpt.com/backend-api/wham/usage")
        .bearer_auth(access_token)
        .header("ChatGPT-Account-Id", account_id)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| QuotaApiError::Other(format!("request failed: {e}")))?;

    if !response.status().is_success() {
        if response.status() == StatusCode::UNAUTHORIZED {
            return Err(QuotaApiError::Unauthorized);
        }
        return Err(QuotaApiError::Other(format!("HTTP {}", response.status())));
    }

    let usage: UsageResponse = response
        .json()
        .await
        .map_err(|e| QuotaApiError::Other(format!("response parse failed: {e}")))?;
    let rate_limit = usage
        .rate_limit
        .ok_or_else(|| QuotaApiError::Other("no rate_limit in response".to_string()))?;
    let windows = parse_windows_from_rate_limits(&rate_limit);
    let credits = parse_credits_from_rate_limits(&rate_limit);
    if windows.is_empty() && credits.is_none() {
        return Err(QuotaApiError::Other(
            "no quota data in rate_limit".to_string(),
        ));
    }

    let summary = summarize_windows(&windows, credits.as_ref());

    Ok(QuotaSnapshot::new(
        Utc::now(),
        Confidence::Exact,
        windows,
        credits,
        summary,
    ))
}
