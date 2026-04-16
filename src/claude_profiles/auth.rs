use anyhow::{Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeOAuthTokens {
    #[serde(rename = "accessToken")]
    pub access_token: String,
    #[serde(rename = "refreshToken")]
    pub refresh_token: String,
    #[serde(rename = "expiresAt")]
    pub expires_at: i64, // Unix timestamp (milliseconds)
    #[serde(default)]
    pub scopes: Vec<String>,
    #[serde(rename = "subscriptionType")]
    pub subscription_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ClaudeAuth {
    #[serde(rename = "claudeAiOauth")]
    pub claude_ai_oauth: Option<ClaudeOAuthTokens>,
}

impl ClaudeAuth {
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        serde_json::from_str(&content)
            .with_context(|| format!("failed to parse {}", path.display()))
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(path, &content)
            .with_context(|| format!("failed to write {}", path.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
                .with_context(|| format!("failed to set permissions on {}", path.display()))?;
        }
        Ok(())
    }

    pub fn access_token(&self) -> Option<&str> {
        self.claude_ai_oauth
            .as_ref()
            .map(|t| t.access_token.as_str())
    }

    pub fn refresh_token(&self) -> Option<&str> {
        self.claude_ai_oauth
            .as_ref()
            .map(|t| t.refresh_token.as_str())
    }

    /// Returns true if the access token expires within the next 5 minutes.
    pub fn is_expiring_soon(&self) -> bool {
        let Some(tokens) = &self.claude_ai_oauth else {
            return true;
        };
        let now_ms = Utc::now().timestamp_millis();
        let margin_ms = 5 * 60 * 1000; // 5 minutes
        tokens.expires_at <= now_ms + margin_ms
    }

    /// Returns true if the access token is currently expired.
    #[allow(dead_code)]
    pub fn is_expired(&self) -> bool {
        let Some(tokens) = &self.claude_ai_oauth else {
            return true;
        };
        let now_ms = Utc::now().timestamp_millis();
        tokens.expires_at <= now_ms
    }

    pub fn subscription_type(&self) -> Option<&str> {
        self.claude_ai_oauth.as_ref()?.subscription_type.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn write_creds(dir: &std::path::Path, json: &str) -> std::path::PathBuf {
        let path = dir.join(".credentials.json");
        fs::write(&path, json).unwrap();
        path
    }

    #[test]
    fn test_load_parses_access_token() {
        let tmp = TempDir::new().unwrap();
        let path = write_creds(
            tmp.path(),
            r#"{"claudeAiOauth":{"accessToken":"tok","refreshToken":"ref","expiresAt":9999999999000,"scopes":[],"subscriptionType":"claude_max"}}"#,
        );
        let auth = ClaudeAuth::load(&path).unwrap();
        assert_eq!(auth.access_token(), Some("tok"));
        assert_eq!(auth.refresh_token(), Some("ref"));
        assert_eq!(auth.subscription_type(), Some("claude_max"));
    }

    #[test]
    fn test_is_expiring_soon_past_expiry() {
        let auth = ClaudeAuth {
            claude_ai_oauth: Some(ClaudeOAuthTokens {
                access_token: "t".into(),
                refresh_token: "r".into(),
                expires_at: 0, // far in the past
                scopes: vec![],
                subscription_type: None,
            }),
        };
        assert!(auth.is_expiring_soon());
    }

    #[test]
    fn test_is_expiring_soon_future_expiry() {
        let future_ms = Utc::now().timestamp_millis() + 60 * 60 * 1000; // 1h from now
        let auth = ClaudeAuth {
            claude_ai_oauth: Some(ClaudeOAuthTokens {
                access_token: "t".into(),
                refresh_token: "r".into(),
                expires_at: future_ms,
                scopes: vec![],
                subscription_type: None,
            }),
        };
        assert!(!auth.is_expiring_soon());
    }

    #[test]
    fn test_save_and_reload() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join(".credentials.json");
        let auth = ClaudeAuth {
            claude_ai_oauth: Some(ClaudeOAuthTokens {
                access_token: "access".into(),
                refresh_token: "refresh".into(),
                expires_at: 9999999999000,
                scopes: vec!["read".into()],
                subscription_type: Some("claude_max".into()),
            }),
        };
        auth.save(&path).unwrap();
        let loaded = ClaudeAuth::load(&path).unwrap();
        assert_eq!(loaded.access_token(), Some("access"));
    }

    #[test]
    fn test_is_expired_past() {
        let auth = ClaudeAuth {
            claude_ai_oauth: Some(ClaudeOAuthTokens {
                access_token: "t".into(),
                refresh_token: "r".into(),
                expires_at: 0,
                scopes: vec![],
                subscription_type: None,
            }),
        };
        assert!(auth.is_expired());
    }

    #[test]
    fn test_is_expired_future() {
        let future_ms = Utc::now().timestamp_millis() + 60 * 60 * 1000;
        let auth = ClaudeAuth {
            claude_ai_oauth: Some(ClaudeOAuthTokens {
                access_token: "t".into(),
                refresh_token: "r".into(),
                expires_at: future_ms,
                scopes: vec![],
                subscription_type: None,
            }),
        };
        assert!(!auth.is_expired());
    }

    #[test]
    fn test_empty_credentials_file() {
        let tmp = TempDir::new().unwrap();
        let path = write_creds(tmp.path(), r#"{}"#);
        let auth = ClaudeAuth::load(&path).unwrap();
        assert!(auth.access_token().is_none());
        assert!(auth.is_expiring_soon()); // None tokens → treats as expiring
    }
}
