use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum Confidence {
    #[default]
    Unavailable,
    Parsed,
    Cached,
    Exact,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum LimitState {
    Hit,
    Available,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[non_exhaustive]
pub enum WindowKind {
    FiveHour,
    Weekly,
    Monthly,
    Daily,
    LongTerm,
    Other(String),
}

impl Default for WindowKind {
    fn default() -> Self {
        Self::Other(String::new())
    }
}

impl WindowKind {
    /// Parse a window-kind label into a typed variant.
    ///
    /// The input is trimmed and **normalised to lowercase** before matching.
    /// Unrecognised labels are stored as-is (lowercased) in the `Other` variant.
    #[must_use]
    pub fn from_label(label: &str) -> Self {
        match label.trim().to_ascii_lowercase().as_str() {
            "5h" | "5h window" | "primary" => Self::FiveHour,
            "weekly" | "7d" | "secondary" => Self::Weekly,
            "monthly" | "30d" => Self::Monthly,
            "daily" | "1d" | "24h" => Self::Daily,
            "long_term" | "long-term" | "longterm" => Self::LongTerm,
            other => Self::Other(other.to_string()),
        }
    }

    pub fn label(&self) -> &str {
        match self {
            Self::FiveHour => "5h",
            Self::Weekly => "weekly",
            Self::Monthly => "monthly",
            Self::Daily => "daily",
            Self::LongTerm => "long-term",
            Self::Other(s) => s.as_str(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct QuotaWindow {
    pub kind: WindowKind,
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
}

impl QuotaSnapshot {
    pub fn new(
        captured_at: DateTime<Utc>,
        confidence: Confidence,
        windows: Vec<QuotaWindow>,
        credits: Option<CreditsState>,
        summary: String,
    ) -> Self {
        Self {
            captured_at,
            confidence,
            windows,
            credits,
            summary,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn from_legacy(
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
        Self::new(
            captured_at,
            confidence,
            vec![
                QuotaWindow {
                    kind: WindowKind::FiveHour,
                    used_pct: five_hour_used_pct,
                    reset_at: five_hour_reset_at,
                    state: five_hour_state,
                },
                QuotaWindow {
                    kind: WindowKind::Weekly,
                    used_pct: weekly_used_pct,
                    reset_at: weekly_reset_at,
                    state: weekly_state,
                },
            ],
            None,
            summary,
        )
    }

    pub fn windows(&self) -> &[QuotaWindow] {
        &self.windows
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct QuotaCache {
    pub by_profile: BTreeMap<String, QuotaSnapshot>,
}
