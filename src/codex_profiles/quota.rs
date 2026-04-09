use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::time::SystemTime;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum Confidence {
    #[default]
    Unavailable,
    Parsed,
    Cached,
    Exact,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum LimitState {
    Hit,
    Available,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct QuotaWindow {
    pub label: String,
    #[serde(default)]
    pub used_pct: Option<u8>,
    #[serde(default)]
    pub reset_at: Option<DateTime<Utc>>,
    pub state: LimitState,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct CreditsState {
    pub enabled: bool,
    #[serde(default)]
    pub balance_label: Option<String>,
    pub unlimited: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct QuotaSnapshot {
    pub captured_at: DateTime<Utc>,
    pub confidence: Confidence,
    #[serde(default)]
    pub windows: Vec<QuotaWindow>,
    #[serde(default)]
    pub credits: Option<CreditsState>,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub five_hour_state: LimitState,
    #[serde(default)]
    pub five_hour_reset_at: Option<DateTime<Utc>>,
    /// Percentage of the 5-hour quota used (0–100). None if not available.
    #[serde(default)]
    pub five_hour_used_pct: Option<u8>,
    #[serde(default)]
    pub weekly_state: LimitState,
    #[serde(default)]
    pub weekly_reset_at: Option<DateTime<Utc>>,
    /// Percentage of the 7-day quota used (0–100). None if not available.
    #[serde(default)]
    pub weekly_used_pct: Option<u8>,
}

impl QuotaSnapshot {
    pub fn from_windows(
        captured_at: DateTime<Utc>,
        confidence: Confidence,
        windows: Vec<QuotaWindow>,
        credits: Option<CreditsState>,
        summary: String,
    ) -> Self {
        let mut snapshot = Self {
            captured_at,
            confidence,
            windows,
            credits,
            summary,
            five_hour_state: LimitState::Unknown,
            five_hour_reset_at: None,
            five_hour_used_pct: None,
            weekly_state: LimitState::Unknown,
            weekly_reset_at: None,
            weekly_used_pct: None,
        };
        snapshot.refresh_legacy_fields();
        snapshot
    }

    pub fn legacy_fallback(
        captured_at: DateTime<Utc>,
        confidence: Confidence,
        five_hour_state: LimitState,
        five_hour_reset_at: Option<DateTime<Utc>>,
        five_hour_used_pct: Option<u8>,
        weekly_state: LimitState,
        weekly_reset_at: Option<DateTime<Utc>>,
        weekly_used_pct: Option<u8>,
        summary: String,
    ) -> Self {
        Self::from_windows(
            captured_at,
            confidence,
            vec![
                QuotaWindow {
                    label: "5h".to_string(),
                    used_pct: five_hour_used_pct,
                    reset_at: five_hour_reset_at,
                    state: five_hour_state.clone(),
                },
                QuotaWindow {
                    label: "weekly".to_string(),
                    used_pct: weekly_used_pct,
                    reset_at: weekly_reset_at,
                    state: weekly_state.clone(),
                },
            ],
            None,
            summary,
        )
    }

    pub fn refresh_legacy_fields(&mut self) {
        if self.windows.is_empty() {
            self.windows = legacy_windows_from_fields(
                self.five_hour_state.clone(),
                self.five_hour_reset_at,
                self.five_hour_used_pct,
                self.weekly_state.clone(),
                self.weekly_reset_at,
                self.weekly_used_pct,
            );
            return;
        }

        let (five_hour, weekly) = legacy_fields_from_windows(&self.windows);
        self.five_hour_state = five_hour.state;
        self.five_hour_reset_at = five_hour.reset_at;
        self.five_hour_used_pct = five_hour.used_pct;
        self.weekly_state = weekly.state;
        self.weekly_reset_at = weekly.reset_at;
        self.weekly_used_pct = weekly.used_pct;
    }

    pub fn effective_windows(&self) -> Vec<QuotaWindow> {
        if self.windows.is_empty() {
            legacy_windows_from_fields(
                self.five_hour_state.clone(),
                self.five_hour_reset_at,
                self.five_hour_used_pct,
                self.weekly_state.clone(),
                self.weekly_reset_at,
                self.weekly_used_pct,
            )
        } else {
            self.windows.clone()
        }
    }
}

fn legacy_windows_from_fields(
    five_hour_state: LimitState,
    five_hour_reset_at: Option<DateTime<Utc>>,
    five_hour_used_pct: Option<u8>,
    weekly_state: LimitState,
    weekly_reset_at: Option<DateTime<Utc>>,
    weekly_used_pct: Option<u8>,
) -> Vec<QuotaWindow> {
    vec![
        QuotaWindow {
            label: "5h".to_string(),
            used_pct: five_hour_used_pct,
            reset_at: five_hour_reset_at,
            state: five_hour_state,
        },
        QuotaWindow {
            label: "weekly".to_string(),
            used_pct: weekly_used_pct,
            reset_at: weekly_reset_at,
            state: weekly_state,
        },
    ]
}

fn legacy_fields_from_windows(windows: &[QuotaWindow]) -> (QuotaWindow, QuotaWindow) {
    let five_hour = windows
        .iter()
        .find(|window| window_label_matches(&window.label, &["5h", "5h window", "primary"]))
        .or_else(|| windows.first())
        .cloned()
        .unwrap_or_default();
    let weekly = windows
        .iter()
        .find(|window| window_label_matches(&window.label, &["weekly", "7d", "secondary"]))
        .or_else(|| windows.get(1))
        .cloned()
        .unwrap_or_default();
    (five_hour, weekly)
}

fn window_label_matches(label: &str, candidates: &[&str]) -> bool {
    let normalized = label.trim().to_ascii_lowercase();
    candidates.iter().any(|candidate| normalized == *candidate)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct QuotaCache {
    pub by_profile: BTreeMap<String, QuotaSnapshot>,
}

/// Deprecated compatibility wrapper for legacy `codex status` text parsing.
#[deprecated(note = "use parse_legacy_status_output for explicit legacy fallback parsing")]
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

    Some(QuotaSnapshot::legacy_fallback(
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

impl QuotaCache {
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }

        let content = fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;

        let mut cache: Self = serde_json::from_str(&content)
            .with_context(|| format!("failed to parse {}", path.display()))?;
        cache.normalize();
        Ok(cache)
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

    pub fn normalize(&mut self) {
        for snapshot in self.by_profile.values_mut() {
            snapshot.refresh_legacy_fields();
        }
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
    let windows = parse_windows_from_rate_limits(rate_limits);
    let credits = parse_credits_from_rate_limits(rate_limits);
    if windows.is_empty() && credits.is_none() {
        return None;
    }

    let summary = summarize_windows(&windows, credits.as_ref());

    Some(QuotaSnapshot::from_windows(
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
    if let Some(windows) = rate_limits.get("windows").and_then(|value| value.as_array()) {
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
        .and_then(|value| value.as_i64().or_else(|| value.as_f64().map(|secs| secs as i64)))
        .and_then(|secs| DateTime::from_timestamp(secs, 0));

    let state = window
        .get("state")
        .or_else(|| window.get("limit_state"))
        .and_then(|value| value.as_str())
        .and_then(parse_limit_state)
        .unwrap_or_else(|| pct_to_state(used_pct));

    QuotaWindow {
        label,
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
            format!("{}={}", window.label, pct)
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
#[allow(dead_code)]
struct UsageResponse {
    #[serde(rename = "used_units")]
    used_units: Option<i32>,
    quota: Option<i32>,
    #[serde(rename = "rate_limit")]
    rate_limit: Option<serde_json::Value>,
    #[serde(rename = "account_id")]
    account_id: Option<String>,
    email: Option<String>,
    #[serde(rename = "plan_type")]
    plan_type: Option<String>,
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
    let windows = parse_windows_from_rate_limits(&rate_limit);
    let credits = parse_credits_from_rate_limits(&rate_limit);
    if windows.is_empty() && credits.is_none() {
        return None;
    }

    let summary = summarize_windows(&windows, credits.as_ref());

    Some(QuotaSnapshot::from_windows(
        Utc::now(),
        Confidence::Exact,
        windows,
        credits,
        summary,
    ))
}
