use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

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
    pub weekly_state: LimitState,
    pub weekly_reset_at: Option<DateTime<Utc>>,
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
        weekly_state,
        weekly_reset_at,
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
