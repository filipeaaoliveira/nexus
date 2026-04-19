use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

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
    /// Percentage of the 5-hour quota used (0-100). None if not available.
    #[serde(default)]
    pub five_hour_used_pct: Option<u8>,
    #[serde(default)]
    pub weekly_state: LimitState,
    #[serde(default)]
    pub weekly_reset_at: Option<DateTime<Utc>>,
    /// Percentage of the 7-day quota used (0-100). None if not available.
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

    #[allow(clippy::too_many_arguments)]
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
                    state: five_hour_state,
                },
                QuotaWindow {
                    label: "weekly".to_string(),
                    used_pct: weekly_used_pct,
                    reset_at: weekly_reset_at,
                    state: weekly_state,
                },
            ],
            None,
            summary,
        )
    }

    pub fn refresh_legacy_fields(&mut self) {
        if self.windows.is_empty() {
            self.windows = legacy_windows_from_fields(
                self.five_hour_state,
                self.five_hour_reset_at,
                self.five_hour_used_pct,
                self.weekly_state,
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
                self.five_hour_state,
                self.five_hour_reset_at,
                self.five_hour_used_pct,
                self.weekly_state,
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

impl QuotaCache {
    pub fn normalize(&mut self) {
        for snapshot in self.by_profile.values_mut() {
            snapshot.refresh_legacy_fields();
        }
    }
}
