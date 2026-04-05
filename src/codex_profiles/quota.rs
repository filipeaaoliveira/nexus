use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::time::SystemTime;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Confidence {
    Exact,
    Parsed,
    Cached,
    Unavailable,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum LimitState {
    Hit,
    Available,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct QuotaSnapshot {
    pub captured_at: DateTime<Utc>,
    pub confidence: Confidence,
    pub five_hour_state: LimitState,
    pub five_hour_reset_at: Option<DateTime<Utc>>,
    /// Percentage of the 5-hour quota used (0–100). None if not available.
    #[serde(default)]
    pub five_hour_used_pct: Option<u8>,
    pub weekly_state: LimitState,
    pub weekly_reset_at: Option<DateTime<Utc>>,
    /// Percentage of the 7-day quota used (0–100). None if not available.
    #[serde(default)]
    pub weekly_used_pct: Option<u8>,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct QuotaCache {
    pub by_profile: BTreeMap<String, QuotaSnapshot>,
}

pub fn parse_status_output(input: &str) -> Option<QuotaSnapshot> {
    let (five_hour_state, five_hour_reset_at, five_hour_seen) = parse_window(input, "5h window");
    let (weekly_state, weekly_reset_at, weekly_seen) = parse_window(input, "weekly window");

    if !five_hour_seen && !weekly_seen {
        return None;
    }

    let summary = format!("5h={:?}; weekly={:?}", five_hour_state, weekly_state);

    Some(QuotaSnapshot {
        captured_at: Utc::now(),
        confidence: Confidence::Parsed,
        five_hour_state,
        five_hour_reset_at,
        five_hour_used_pct: None,
        weekly_state,
        weekly_reset_at,
        weekly_used_pct: None,
        summary,
    })
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

impl QuotaCache {
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }

        let content = fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;

        serde_json::from_str(&content)
            .with_context(|| format!("failed to parse {}", path.display()))
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }

        let content = serde_json::to_string_pretty(self)?;
        fs::write(path, content).with_context(|| format!("failed to write {}", path.display()))?;
        Ok(())
    }
}

pub fn load_quota_cache(path: &Path) -> Result<QuotaCache> {
    QuotaCache::load(path)
}

pub fn save_quota_cache(cache: &QuotaCache, path: &Path) -> Result<()> {
    cache.save(path)
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
        .filter(|obj| {
            obj.get("type").and_then(|v| v.as_str()) == Some("event_msg")
                && obj.pointer("/payload/type").and_then(|v| v.as_str()) == Some("token_count")
        })
        .last()?;

    let rate_limits = last_event.pointer("/payload/rate_limits")?;
    let primary = rate_limits.get("primary")?;
    let secondary = rate_limits.get("secondary")?;

    let five_hour_pct_raw = primary
        .get("used_percent")
        .and_then(|v| v.as_f64())
        .map(|f| f.round() as u8);
    let weekly_pct_raw = secondary
        .get("used_percent")
        .and_then(|v| v.as_f64())
        .map(|f| f.round() as u8);

    let five_hour_state = pct_to_state(five_hour_pct_raw);
    let weekly_state = pct_to_state(weekly_pct_raw);

    let five_hour_reset_at = primary
        .get("resets_at")
        .and_then(|v| v.as_i64())
        .and_then(|secs| DateTime::from_timestamp(secs, 0));
    let weekly_reset_at = secondary
        .get("resets_at")
        .and_then(|v| v.as_i64())
        .and_then(|secs| DateTime::from_timestamp(secs, 0));

    let summary = format!(
        "5h={}%; weekly={}%",
        five_hour_pct_raw
            .map(|p| p.to_string())
            .unwrap_or_else(|| "?".to_string()),
        weekly_pct_raw
            .map(|p| p.to_string())
            .unwrap_or_else(|| "?".to_string()),
    );

    Some(QuotaSnapshot {
        captured_at: Utc::now(),
        confidence: Confidence::Parsed,
        five_hour_state,
        five_hour_reset_at,
        five_hour_used_pct: five_hour_pct_raw,
        weekly_state,
        weekly_reset_at,
        weekly_used_pct: weekly_pct_raw,
        summary,
    })
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

use crate::codex_profiles::ProfileAuth;

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct UsageResponse {
    #[serde(rename = "used_units")]
    used_units: Option<i32>,
    quota: Option<i32>,
    #[serde(rename = "rate_limit")]
    rate_limit: Option<RateLimitResponse>,
    #[serde(rename = "account_id")]
    account_id: Option<String>,
    email: Option<String>,
    #[serde(rename = "plan_type")]
    plan_type: Option<String>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct RateLimitResponse {
    #[serde(rename = "primary_window")]
    primary_window: Option<WindowResponse>,
    #[serde(rename = "secondary_window")]
    secondary_window: Option<WindowResponse>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct WindowResponse {
    #[serde(rename = "used_percent")]
    used_percent: Option<f64>,
    name: Option<String>,
    #[serde(rename = "reset_at")]
    reset_at: Option<f64>,
}

pub async fn fetch_quota_from_api(profile_auth: &ProfileAuth) -> Option<QuotaSnapshot> {
    let account_id = profile_auth.account_id()?;
    let access_token = profile_auth.access_token()?;
    fetch_quota_async(account_id, access_token).await
}

/// Spawns an async task to fetch quota in the background.
/// When complete, calls the callback with the result.
/// Returns a handle that can be awaited.
pub fn spawn_quota_fetch<F>(profile_id: String, auth: ProfileAuth, on_done: F)
where
    F: FnOnce(String, Option<QuotaSnapshot>) + Send + 'static,
{
    tokio::spawn(async move {
        let result = fetch_quota_from_api(&auth).await;
        on_done(profile_id, result);
    });
}

/// Sync wrapper that runs the async fetch in a blocking context.
/// Used when calling from synchronous code.
pub fn fetch_quota_from_api_sync(profile_auth: &ProfileAuth) -> Option<QuotaSnapshot> {
    let account_id = match profile_auth.account_id() {
        Some(id) => id.to_string(),
        None => return None,
    };
    let access_token = match profile_auth.access_token() {
        Some(token) => token.to_string(),
        None => return None,
    };
    
    // Try to get current runtime, or create a new one
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        handle.block_on(fetch_quota_async(&account_id, &access_token))
    } else {
        // Create a runtime for tests or when no runtime exists
        let rt = tokio::runtime::Runtime::new().ok()?;
        rt.block_on(fetch_quota_async(&account_id, &access_token))
    }
}

async fn fetch_quota_async(account_id: &str, access_token: &str) -> Option<QuotaSnapshot> {
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .ok()?;

    let response = client
        .get("https://chatgpt.com/backend-api/wham/usage")
        .bearer_auth(access_token)
        .header("ChatGPT-Account-Id", account_id)
        .header("Accept", "application/json")
        .send()
        .await
        .ok()?;

    if !response.status().is_success() {
        return None;
    }

    let usage: UsageResponse = response.json().await.ok()?;
    let rate_limit = usage.rate_limit?;
    let primary = rate_limit.primary_window;
    let secondary = rate_limit.secondary_window;

    let five_hour_pct_raw = primary
        .as_ref()
        .and_then(|w| w.used_percent)
        .map(|f| f.round() as u8);
    let weekly_pct_raw = secondary
        .as_ref()
        .and_then(|w| w.used_percent)
        .map(|f| f.round() as u8);

    let five_hour_state = pct_to_state(five_hour_pct_raw);
    let weekly_state = pct_to_state(weekly_pct_raw);

    let five_hour_reset_at = primary
        .as_ref()
        .and_then(|w| w.reset_at)
        .and_then(|secs| DateTime::from_timestamp(secs as i64, 0));
    let weekly_reset_at = secondary
        .as_ref()
        .and_then(|w| w.reset_at)
        .and_then(|secs| DateTime::from_timestamp(secs as i64, 0));

    let summary = format!(
        "5h={}%; weekly={}%",
        five_hour_pct_raw
            .map(|p| p.to_string())
            .unwrap_or_else(|| "?".to_string()),
        weekly_pct_raw
            .map(|p| p.to_string())
            .unwrap_or_else(|| "?".to_string()),
    );

    Some(QuotaSnapshot {
        captured_at: Utc::now(),
        confidence: Confidence::Exact,
        five_hour_state,
        five_hour_reset_at,
        five_hour_used_pct: five_hour_pct_raw,
        weekly_state,
        weekly_reset_at,
        weekly_used_pct: weekly_pct_raw,
        summary,
    })
}
