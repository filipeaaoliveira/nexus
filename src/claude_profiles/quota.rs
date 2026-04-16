use chrono::DateTime;
use reqwest::Client;
use serde::Deserialize;

use crate::claude_profiles::auth::ClaudeAuth;
use crate::domain::accounts::{Confidence, LimitState, QuotaSnapshot, QuotaWindow};

#[derive(Debug, Deserialize)]
struct ClaudeUsageResponse {
    rate_limits: Option<ClaudeRateLimits>,
}

#[derive(Debug, Deserialize)]
struct ClaudeRateLimits {
    primary: Option<ClaudeWindow>,
    secondary: Option<ClaudeWindow>,
}

#[derive(Debug, Deserialize)]
struct ClaudeWindow {
    used_percent: Option<f64>,
    resets_at: Option<i64>, // Unix timestamp seconds
}

#[derive(Debug, Deserialize)]
struct TokenRefreshResponse {
    #[serde(rename = "accessToken")]
    access_token: Option<String>,
    /// Unix timestamp in milliseconds — same unit as ClaudeOAuthTokens::expires_at.
    /// Anthropic's OAuth endpoints use the same credential format as .credentials.json.
    #[serde(rename = "expiresAt")]
    expires_at: Option<i64>,
}

/// Refresh the OAuth access token if it is expiring soon.
/// Returns the updated `ClaudeAuth` on success (caller must persist it).
/// Returns the original `auth` unchanged if refresh is not needed or fails.
pub async fn refresh_token_if_needed(auth: ClaudeAuth) -> ClaudeAuth {
    if !auth.is_expiring_soon() {
        return auth;
    }
    let Some(refresh_token) = auth.refresh_token() else {
        return auth;
    };
    let client = match Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
    {
        Ok(c) => c,
        Err(_) => return auth,
    };

    let body = serde_json::json!({
        "grant_type": "refresh_token",
        "refresh_token": refresh_token
    });

    let resp = client
        .post("https://api.anthropic.com/v1/oauth/token")
        .json(&body)
        .send()
        .await;

    let Ok(resp) = resp else { return auth };
    if !resp.status().is_success() {
        return auth;
    }
    let Ok(refreshed) = resp.json::<TokenRefreshResponse>().await else {
        return auth;
    };

    let Some(new_access) = refreshed.access_token else {
        return auth;
    };
    let mut updated = auth;
    if let Some(ref mut tokens) = updated.claude_ai_oauth {
        tokens.access_token = new_access;
        if let Some(expires_at) = refreshed.expires_at {
            tokens.expires_at = expires_at;
        }
    }
    updated
}

pub async fn fetch_claude_quota(auth: &ClaudeAuth) -> Result<QuotaSnapshot, String> {
    let access_token = auth
        .access_token()
        .ok_or("missing access_token in credentials")?;
    fetch_claude_quota_async(access_token).await
}

async fn fetch_claude_quota_async(access_token: &str) -> Result<QuotaSnapshot, String> {
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("client build failed: {e}"))?;

    let response = client
        .get("https://api.anthropic.com/api/oauth/usage")
        .bearer_auth(access_token)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| format!("request failed: {e}"))?;

    if !response.status().is_success() {
        return Err(format!("HTTP {}", response.status()));
    }

    let usage: ClaudeUsageResponse = response
        .json()
        .await
        .map_err(|e| format!("response parse failed: {e}"))?;
    let rate_limits = usage.rate_limits.ok_or("no rate_limits in response")?;
    let windows = [
        rate_limits
            .primary
            .as_ref()
            .map(|window| window_response_to_quota_window("5h", window)),
        rate_limits
            .secondary
            .as_ref()
            .map(|window| window_response_to_quota_window("weekly", window)),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>();

    let summary = summarize_windows(&windows);

    Ok(QuotaSnapshot::from_windows(
        chrono::Utc::now(),
        Confidence::Exact,
        windows,
        None,
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

fn window_response_to_quota_window(label: &str, window: &ClaudeWindow) -> QuotaWindow {
    let used_pct = window
        .used_percent
        .map(|f| f.round().clamp(0.0, 100.0) as u8);
    let state = pct_to_state(used_pct);
    let reset_at = window
        .resets_at
        .and_then(|secs| DateTime::from_timestamp(secs, 0));

    QuotaWindow {
        label: label.to_string(),
        used_pct,
        reset_at,
        state,
    }
}

fn summarize_windows(windows: &[QuotaWindow]) -> String {
    windows
        .iter()
        .map(|window| {
            let pct = window
                .used_pct
                .map(|value| format!("{value}%"))
                .unwrap_or_else(|| "?".to_string());
            format!("{}={}", window.label, pct)
        })
        .collect::<Vec<_>>()
        .join("; ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pct_to_state_hit() {
        assert_eq!(pct_to_state(Some(100)), LimitState::Hit);
        assert_eq!(pct_to_state(Some(101)), LimitState::Hit);
    }

    #[test]
    fn test_pct_to_state_available() {
        assert_eq!(pct_to_state(Some(99)), LimitState::Available);
        assert_eq!(pct_to_state(Some(0)), LimitState::Available);
    }

    #[test]
    fn test_pct_to_state_unknown() {
        assert_eq!(pct_to_state(None), LimitState::Unknown);
    }
}
