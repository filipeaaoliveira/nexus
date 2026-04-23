use anyhow::{Context, Result};
use base64::Engine;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::Path;

const ACCESS_TOKEN_REFRESH_MARGIN_SECS: i64 = 10 * 60;
const LAST_REFRESH_MAX_AGE_SECS: i64 = 9 * 24 * 60 * 60;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileAuth {
    pub auth_mode: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tokens: Option<Tokens>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_id: Option<String>,
    #[serde(
        default,
        rename = "OPENAI_API_KEY",
        skip_serializing_if = "Option::is_none"
    )]
    pub openai_api_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_refresh: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tokens {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub access_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id_token: Option<String>,
    #[serde(
        default,
        rename = "account_id",
        skip_serializing_if = "Option::is_none"
    )]
    pub account_id: Option<String>,
}

/// The OAuth client ID used by the Codex CLI for ChatGPT authentication.
const OPENAI_OAUTH_CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";

impl ProfileAuth {
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
        let mut json = serde_json::to_value(self)?;
        if let Ok(content) = std::fs::read_to_string(path) {
            if let Ok(existing) = serde_json::from_str::<serde_json::Value>(&content) {
                preserve_unknown_auth_fields(&existing, &mut json);
            }
        }
        let content = serde_json::to_string_pretty(&json)?;
        atomic_write(path, content.as_bytes())
            .with_context(|| format!("failed to write {}", path.display()))?;
        Ok(())
    }

    pub fn account_id(&self) -> Option<&str> {
        self.tokens
            .as_ref()
            .and_then(|t| t.account_id.as_deref())
            .or(self.account_id.as_deref())
    }

    pub fn access_token(&self) -> Option<&str> {
        self.tokens.as_ref().and_then(|t| t.access_token.as_deref())
    }

    pub fn refresh_token(&self) -> Option<&str> {
        self.tokens
            .as_ref()
            .and_then(|t| t.refresh_token.as_deref())
    }

    pub fn access_token_expires_at(&self) -> Option<DateTime<Utc>> {
        let token = self.access_token()?;
        let payload = token.split('.').nth(1)?;
        let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(payload)
            .ok()?;
        let payload: serde_json::Value = serde_json::from_slice(&decoded).ok()?;
        let exp = payload.get("exp")?.as_i64()?;
        DateTime::from_timestamp(exp, 0)
    }

    pub fn should_refresh_access_token(&self, now: DateTime<Utc>) -> bool {
        if self.access_token().is_none() {
            return true;
        }

        if let Some(expires_at) = self.access_token_expires_at() {
            return expires_at <= now + chrono::Duration::seconds(ACCESS_TOKEN_REFRESH_MARGIN_SECS);
        }

        let Some(last_refresh) = self.last_refresh else {
            return true;
        };
        now.signed_duration_since(last_refresh).num_seconds() >= LAST_REFRESH_MAX_AGE_SECS
    }

    /// Returns true if the access token was last refreshed more than 9 days ago
    /// (tokens expire after 10 days), or if there is no refresh timestamp.
    #[cfg(test)]
    pub fn is_expiring_soon(&self) -> bool {
        self.should_refresh_access_token(Utc::now())
    }

    pub fn oauth_client_id() -> &'static str {
        OPENAI_OAUTH_CLIENT_ID
    }
}

fn preserve_unknown_auth_fields(existing: &serde_json::Value, next: &mut serde_json::Value) {
    const KNOWN_AUTH_FIELDS: &[&str] = &[
        "auth_mode",
        "tokens",
        "account_id",
        "OPENAI_API_KEY",
        "last_refresh",
    ];
    const KNOWN_TOKEN_FIELDS: &[&str] =
        &["access_token", "refresh_token", "id_token", "account_id"];

    preserve_unknown_object_fields(existing, next, KNOWN_AUTH_FIELDS);

    if let (Some(existing_tokens), Some(next_tokens)) =
        (existing.get("tokens"), next.get_mut("tokens"))
    {
        preserve_unknown_object_fields(existing_tokens, next_tokens, KNOWN_TOKEN_FIELDS);
    }
}

fn preserve_unknown_object_fields(
    existing: &serde_json::Value,
    next: &mut serde_json::Value,
    known_fields: &[&str],
) {
    let (Some(existing), Some(next)) = (existing.as_object(), next.as_object_mut()) else {
        return;
    };

    for (key, value) in existing {
        if !known_fields.contains(&key.as_str()) {
            next.entry(key.clone()).or_insert_with(|| value.clone());
        }
    }
}

fn atomic_write(path: &Path, content: &[u8]) -> Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("auth.json");
    let tmp_path = parent.join(format!(
        ".{file_name}.tmp-{}-{}",
        std::process::id(),
        Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));

    let result = (|| -> Result<()> {
        #[cfg(unix)]
        let mut file = {
            use std::os::unix::fs::OpenOptionsExt;
            std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&tmp_path)
                .with_context(|| format!("failed to create {}", tmp_path.display()))?
        };
        #[cfg(not(unix))]
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp_path)
            .with_context(|| format!("failed to create {}", tmp_path.display()))?;
        file.write_all(content)
            .with_context(|| format!("failed to write {}", tmp_path.display()))?;
        file.sync_all()
            .with_context(|| format!("failed to sync {}", tmp_path.display()))?;
        std::fs::rename(&tmp_path, path)
            .with_context(|| format!("failed to replace {}", path.display()))?;
        Ok(())
    })();

    if result.is_err() {
        let _ = std::fs::remove_file(&tmp_path);
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;
    use std::fs;
    use tempfile::TempDir;

    fn write_auth(dir: &std::path::Path, json: &str) -> std::path::PathBuf {
        let path = dir.join("auth.json");
        fs::write(&path, json).unwrap();
        path
    }

    fn jwt_with_exp(exp: i64) -> String {
        let header = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(r#"{"alg":"none"}"#);
        let payload =
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(format!(r#"{{"exp":{exp}}}"#));
        format!("{header}.{payload}.")
    }

    #[test]
    fn load_parses_access_and_refresh_tokens() {
        let tmp = TempDir::new().unwrap();
        let path = write_auth(
            tmp.path(),
            r#"{"auth_mode":"chatgpt","tokens":{"access_token":"acc","refresh_token":"ref","account_id":"acc-id"}}"#,
        );
        let auth = ProfileAuth::load(&path).unwrap();
        assert_eq!(auth.access_token(), Some("acc"));
        assert_eq!(auth.refresh_token(), Some("ref"));
        assert_eq!(auth.account_id(), Some("acc-id"));
    }

    #[test]
    fn load_handles_missing_refresh_token() {
        let tmp = TempDir::new().unwrap();
        let path = write_auth(
            tmp.path(),
            r#"{"auth_mode":"chatgpt","tokens":{"access_token":"acc","account_id":"acc-id"}}"#,
        );
        let auth = ProfileAuth::load(&path).unwrap();
        assert_eq!(auth.access_token(), Some("acc"));
        assert!(auth.refresh_token().is_none());
    }

    #[test]
    fn is_expiring_soon_returns_true_when_no_last_refresh() {
        let auth = ProfileAuth {
            auth_mode: "chatgpt".into(),
            tokens: None,
            account_id: None,
            openai_api_key: None,
            last_refresh: None,
        };
        assert!(auth.is_expiring_soon());
    }

    #[test]
    fn is_expiring_soon_returns_true_when_last_refresh_older_than_9_days() {
        let old = Utc::now() - chrono::Duration::days(10);
        let auth = ProfileAuth {
            auth_mode: "chatgpt".into(),
            tokens: None,
            account_id: None,
            openai_api_key: None,
            last_refresh: Some(old),
        };
        assert!(auth.is_expiring_soon());
    }

    #[test]
    fn is_expiring_soon_returns_false_when_recently_refreshed() {
        let recent = Utc::now() - chrono::Duration::hours(1);
        let auth = ProfileAuth {
            auth_mode: "chatgpt".into(),
            tokens: Some(Tokens {
                access_token: Some("opaque-access-token".into()),
                refresh_token: Some("ref".into()),
                id_token: None,
                account_id: Some("acc-id".into()),
            }),
            account_id: None,
            openai_api_key: None,
            last_refresh: Some(recent),
        };
        assert!(!auth.is_expiring_soon());
    }

    #[test]
    fn should_refresh_access_token_uses_jwt_exp_when_present() {
        let now = DateTime::from_timestamp(1_700_000_000, 0).unwrap();
        let auth = ProfileAuth {
            auth_mode: "chatgpt".into(),
            tokens: Some(Tokens {
                access_token: Some(jwt_with_exp(1_700_000_120)),
                refresh_token: Some("ref".into()),
                id_token: None,
                account_id: Some("acc-id".into()),
            }),
            account_id: None,
            openai_api_key: None,
            last_refresh: Some(now),
        };

        assert!(auth.should_refresh_access_token(now));
    }

    #[test]
    fn should_refresh_access_token_keeps_valid_jwt_without_using_stale_last_refresh() {
        let now = DateTime::from_timestamp(1_700_000_000, 0).unwrap();
        let auth = ProfileAuth {
            auth_mode: "chatgpt".into(),
            tokens: Some(Tokens {
                access_token: Some(jwt_with_exp(1_700_086_400)),
                refresh_token: Some("ref".into()),
                id_token: None,
                account_id: Some("acc-id".into()),
            }),
            account_id: None,
            openai_api_key: None,
            last_refresh: Some(now - chrono::Duration::days(30)),
        };

        assert!(!auth.should_refresh_access_token(now));
    }

    #[test]
    fn should_refresh_access_token_returns_true_when_access_token_is_missing() {
        let now = DateTime::from_timestamp(1_700_000_000, 0).unwrap();
        let auth = ProfileAuth {
            auth_mode: "chatgpt".into(),
            tokens: Some(Tokens {
                access_token: None,
                refresh_token: Some("ref".into()),
                id_token: None,
                account_id: Some("acc-id".into()),
            }),
            account_id: None,
            openai_api_key: None,
            last_refresh: Some(now),
        };

        assert!(auth.should_refresh_access_token(now));
    }

    #[test]
    fn save_and_load_round_trips_all_fields() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("auth.json");
        use chrono::Timelike;
        let now = Utc::now().with_nanosecond(0).unwrap();
        let auth = ProfileAuth {
            auth_mode: "chatgpt".into(),
            tokens: Some(Tokens {
                access_token: Some("acc".into()),
                refresh_token: Some("ref".into()),
                id_token: Some("id".into()),
                account_id: Some("acc-id".into()),
            }),
            account_id: None,
            openai_api_key: None,
            last_refresh: Some(now),
        };
        auth.save(&path).unwrap();
        let loaded = ProfileAuth::load(&path).unwrap();
        assert_eq!(loaded.access_token(), Some("acc"));
        assert_eq!(loaded.refresh_token(), Some("ref"));
        assert_eq!(loaded.account_id(), Some("acc-id"));
        assert_eq!(loaded.last_refresh, Some(now));
    }

    #[test]
    fn save_preserves_unknown_auth_fields_from_existing_file() {
        let tmp = TempDir::new().unwrap();
        let path = write_auth(
            tmp.path(),
            r#"{"auth_mode":"chatgpt","tokens":{"access_token":"old","refresh_token":"old-ref","account_id":"acc-id","future_token_field":{"keep":true}},"future_top_field":{"keep":"yes"}}"#,
        );
        let mut auth = ProfileAuth::load(&path).unwrap();
        auth.tokens.as_mut().unwrap().access_token = Some("new".into());
        auth.tokens.as_mut().unwrap().refresh_token = Some("new-ref".into());

        auth.save(&path).unwrap();

        let saved: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(saved["future_top_field"]["keep"], "yes");
        assert_eq!(saved["tokens"]["future_token_field"]["keep"], true);
        assert_eq!(saved["tokens"]["access_token"], "new");
        assert_eq!(saved["tokens"]["refresh_token"], "new-ref");
    }

    #[test]
    fn save_omits_null_fields_from_output() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("auth.json");
        let auth = ProfileAuth {
            auth_mode: "chatgpt".into(),
            tokens: Some(Tokens {
                access_token: Some("acc".into()),
                refresh_token: None,
                id_token: None,
                account_id: Some("acc-id".into()),
            }),
            account_id: None,
            openai_api_key: None,
            last_refresh: None,
        };
        auth.save(&path).unwrap();
        let content = fs::read_to_string(&path).unwrap();
        assert!(
            !content.contains("null"),
            "null fields must not appear in saved auth.json"
        );
        assert!(
            !content.contains("OPENAI_API_KEY"),
            "absent optional fields must not appear"
        );
        assert!(
            !content.contains("last_refresh"),
            "absent optional fields must not appear"
        );
    }
}
