use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct AuthMetadata {
    pub email: Option<String>,
    pub plan: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StoredCodexProfile {
    pub id: String,
    pub label: String,
    pub email: Option<String>,
    pub plan: Option<String>,
    pub notes: Option<String>,
    pub enabled: bool,
    pub last_used: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub profile_dir: PathBuf,
    pub auth_path: PathBuf,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileMetadata {
    pub id: String,
    pub label: Option<String>,
    pub email: Option<String>,
    pub plan: Option<String>,
    pub notes: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

impl Default for ProfileMetadata {
    fn default() -> Self {
        Self {
            id: String::new(),
            label: None,
            email: None,
            plan: None,
            notes: None,
            enabled: true,
            created_at: None,
            updated_at: None,
        }
    }
}

impl From<StoredCodexProfile> for crate::domain::accounts::DomainProfile {
    fn from(p: StoredCodexProfile) -> Self {
        Self {
            id: crate::domain::accounts::ProfileId::new(p.id),
            provider: crate::domain::accounts::Provider::Codex,
            label: p.label,
            email: p.email,
            plan: p.plan,
            notes: p.notes,
            enabled: p.enabled,
            created_at: p.created_at,
            updated_at: p.updated_at,
        }
    }
}

impl StoredCodexProfile {
    pub fn import_from_auth(
        id: &str,
        label: &str,
        profile_dir: PathBuf,
        auth_source: &Path,
    ) -> Result<Self> {
        if id.trim().is_empty() {
            bail!("id cannot be empty or whitespace");
        }
        if label.trim().is_empty() {
            bail!("label cannot be empty or whitespace");
        }

        let content = fs::read_to_string(auth_source)
            .with_context(|| format!("failed to read {}", auth_source.display()))?;
        let json: Value = serde_json::from_str(&content).context("invalid auth.json")?;
        let metadata = extract_auth_metadata(&json);

        fs::create_dir_all(&profile_dir)
            .with_context(|| format!("failed to create {}", profile_dir.display()))?;

        let auth_path = profile_dir.join("auth.json");
        fs::copy(auth_source, &auth_path).with_context(|| {
            format!(
                "failed to copy {} to {}",
                auth_source.display(),
                auth_path.display()
            )
        })?;

        let now = Utc::now();

        Ok(Self {
            id: id.to_string(),
            label: label.to_string(),
            email: metadata.email,
            plan: metadata.plan,
            notes: None,
            enabled: true,
            last_used: None,
            created_at: now,
            updated_at: now,
            profile_dir,
            auth_path,
        })
    }
}

fn decode_jwt_payload(token: &str) -> Option<Value> {
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() < 2 {
        return None;
    }

    let payload = parts[1];

    // URL_SAFE_NO_PAD accepts raw base64url without padding characters.
    // Do NOT add '=' padding — the NO_PAD engine rejects those characters.
    use base64::Engine;
    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .ok()?;

    serde_json::from_slice(&decoded).ok()
}

pub fn extract_auth_metadata(value: &Value) -> AuthMetadata {
    let email = value
        .get("email")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            let tokens = value.get("tokens")?;
            let id_token = tokens.get("id_token")?.as_str()?;
            let payload = decode_jwt_payload(id_token)?;
            payload.get("email")?.as_str().map(str::to_string)
        });

    let plan = value
        .get("plan")
        .or_else(|| value.get("subscription"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            let tokens = value.get("tokens")?;
            let id_token = tokens.get("id_token")?.as_str()?;
            let payload = decode_jwt_payload(id_token)?;
            let auth = payload.get("https://api.openai.com/auth")?;
            auth.get("chatgpt_plan_type")?.as_str().map(str::to_string)
        });

    AuthMetadata { email, plan }
}

pub fn load_profile_metadata(profile_dir: &Path) -> Result<ProfileMetadata> {
    let metadata_path = profile_dir.join("profile.json");
    if !metadata_path.is_file() {
        return Ok(ProfileMetadata::default());
    }
    let content = fs::read_to_string(&metadata_path)
        .with_context(|| format!("failed to read {}", metadata_path.display()))?;
    let metadata: ProfileMetadata = serde_json::from_str(&content)
        .with_context(|| format!("failed to parse {}", metadata_path.display()))?;
    Ok(metadata)
}

pub fn save_profile_metadata(profile_dir: &Path, metadata: &ProfileMetadata) -> Result<()> {
    let metadata_path = profile_dir.join("profile.json");
    let content = serde_json::to_string_pretty(metadata)?;
    fs::write(&metadata_path, content)
        .with_context(|| format!("failed to write {}", metadata_path.display()))?;
    Ok(())
}

/// Detects which stored profile is currently "live" by comparing
/// `~/.codex/auth.json` bytes against each profile's `auth.json`.
/// Returns the profile ID of the first match, or `None` if no match
/// or if `codex_home_auth` cannot be read.
pub fn detect_live_profile_id(profiles_dir: &Path, codex_home_auth: &Path) -> Option<String> {
    let home_bytes = fs::read(codex_home_auth).ok()?;

    let entries = fs::read_dir(profiles_dir).ok()?;
    let mut ids: Vec<String> = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();
    ids.sort();

    for id in ids {
        let profile_auth = profiles_dir.join(&id).join("auth.json");
        if let Ok(profile_bytes) = fs::read(&profile_auth) {
            if profile_bytes == home_bytes {
                return Some(id);
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn write_file(dir: &std::path::Path, name: &str, content: &[u8]) {
        fs::write(dir.join(name), content).unwrap();
    }

    #[test]
    fn test_detect_live_profile_id_match() {
        let tmp = TempDir::new().unwrap();
        let profiles_dir = tmp.path().join("profiles");
        let p1 = profiles_dir.join("profile-01");
        fs::create_dir_all(&p1).unwrap();
        write_file(&p1, "auth.json", b"{\"email\":\"a@b.com\"}");

        let home_auth = tmp.path().join("auth.json");
        fs::write(&home_auth, b"{\"email\":\"a@b.com\"}").unwrap();

        let result = detect_live_profile_id(&profiles_dir, &home_auth);
        assert_eq!(result, Some("profile-01".to_string()));
    }

    #[test]
    fn test_detect_live_profile_id_no_match() {
        let tmp = TempDir::new().unwrap();
        let profiles_dir = tmp.path().join("profiles");
        let p1 = profiles_dir.join("profile-01");
        fs::create_dir_all(&p1).unwrap();
        write_file(&p1, "auth.json", b"{\"email\":\"a@b.com\"}");

        let home_auth = tmp.path().join("auth.json");
        fs::write(&home_auth, b"{\"email\":\"other@b.com\"}").unwrap();

        let result = detect_live_profile_id(&profiles_dir, &home_auth);
        assert_eq!(result, None);
    }

    #[test]
    fn test_detect_live_profile_id_missing_home_auth() {
        let tmp = TempDir::new().unwrap();
        let profiles_dir = tmp.path().join("profiles");
        let p1 = profiles_dir.join("profile-01");
        fs::create_dir_all(&p1).unwrap();
        write_file(&p1, "auth.json", b"{\"email\":\"a@b.com\"}");

        let home_auth = tmp.path().join("nonexistent-auth.json");

        let result = detect_live_profile_id(&profiles_dir, &home_auth);
        assert_eq!(result, None);
    }

    #[test]
    fn test_detect_live_profile_id_empty_profiles_dir() {
        let tmp = TempDir::new().unwrap();
        let profiles_dir = tmp.path().join("profiles");
        fs::create_dir_all(&profiles_dir).unwrap();

        let home_auth = tmp.path().join("auth.json");
        write_file(tmp.path(), "auth.json", b"{\"email\":\"a@b.com\"}");

        let result = detect_live_profile_id(&profiles_dir, &home_auth);
        assert_eq!(result, None);
    }

    #[test]
    fn test_profile_metadata_default_enabled_true() {
        let m = ProfileMetadata::default();
        assert!(m.enabled, "default ProfileMetadata must have enabled=true");
    }

    #[test]
    fn test_load_profile_metadata_absent_file_enabled_true() {
        let tmp = TempDir::new().unwrap();
        // profile dir exists but no profile.json inside
        let result = load_profile_metadata(tmp.path()).unwrap();
        assert!(
            result.enabled,
            "missing profile.json must yield enabled=true"
        );
    }

    #[test]
    fn test_load_profile_metadata_missing_enabled_field_defaults_true() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("profile.json"), b"{\"id\":\"x\"}").unwrap();
        let result = load_profile_metadata(tmp.path()).unwrap();
        assert!(
            result.enabled,
            "profile.json without enabled field must yield enabled=true"
        );
    }

    // Synthetic JWT token for testing JWT parsing — all fields are fake, no real user data.
    // Payload is 1319 base64url chars (mod4=3), which requires one '=' when padded.
    // URL_SAFE_NO_PAD must decode this correctly without adding padding characters.
    const SYNTHETIC_ID_TOKEN: &str = "eyJhbGciOiJSUzI1NiIsImtpZCI6IjAwMDAwMDAwLTAwMDAtMDAwMC0wMDAwLTAwMDAwMDAwMDAwMCIsInR5cCI6IkpXVCJ9.eyJhbXIiOlsib3RwIiwidXJuOm9wZW5haTphbXI6b3RwX2VtYWlsIl0sImF0X2hhc2giOiJBQUFBQUFBQUFBQUFBQUFBQUFBQUFBIiwiYXVkIjpbImFwcF9BQUFBQUFBQUFBQUFBQUFBQUFBQUFBIl0sImF1dGhfcHJvdmlkZXIiOiJwYXNzd29yZGxlc3MiLCJhdXRoX3RpbWUiOjE3MDAwMDAwMDAsImVtYWlsIjoidGVzdHVzZXJAZXhhbXBsZS5jb20iLCJlbWFpbF92ZXJpZmllZCI6dHJ1ZSwiZXhwIjoxNzAwMDAzNjAwLCJodHRwczovL2FwaS5vcGVuYWkuY29tL2F1dGgiOnsiY2hhdGdwdF9hY2NvdW50X2lkIjoiMDAwMDAwMDAtMDAwMC0wMDAwLTAwMDAtMDAwMDAwMDAwMDAxIiwiY2hhdGdwdF9wbGFuX3R5cGUiOiJwbHVzIiwiY2hhdGdwdF9zdWJzY3JpcHRpb25fYWN0aXZlX3N0YXJ0IjoiMjAyMy0xMS0wMVQwMDowMDowMCswMDowMCIsImNoYXRncHRfc3Vic2NyaXB0aW9uX2FjdGl2ZV91bnRpbCI6IjIwMjMtMTItMDFUMDA6MDA6MDArMDA6MDAiLCJjaGF0Z3B0X3N1YnNjcmlwdGlvbl9sYXN0X2NoZWNrZWQiOiIyMDIzLTExLTE1VDAwOjAwOjAwLjAwMDAwMCswMDowMCIsImNoYXRncHRfdXNlcl9pZCI6InVzZXItQUFBQUFBQUFBQUFBQUFBQUFBQUFBQSIsImdyb3VwcyI6W10sImxvY2FsaG9zdCI6ZmFsc2UsIm9yZ2FuaXphdGlvbnMiOlt7ImlkIjoib3JnLUFBQUFBQUFBQUFBQUFBQUFBQUFBQUEiLCJpc19kZWZhdWx0Ijp0cnVlLCJyb2xlIjoib3duZXIiLCJ0aXRsZSI6IlBlcnNvbmFsIn1dLCJ1c2VyX2lkIjoidXNlci1BQUFBQUFBQUFBQUFBQUFBQUFBQUFBIn0sImlhdCI6MTcwMDAwMDAwMCwiaXNzIjoiaHR0cHM6Ly9hdXRoLm9wZW5haS5jb20iLCJqdGkiOiIwMDAwMDAwMC0wMDAwLTAwMDAtMDAwMC0wMDAwMDAwMDAwMDIiLCJuYW1lIjoiVGVzdCBVc2VyIiwicmF0IjoxNjk5OTk5OTk5LCJzaWQiOiIwMDAwMDAwMC0wMDAwLTAwMDAtMDAwMC0wMDAwMDAwMDAwMDMiLCJzdWIiOiJhdXRoMHx0ZXN0X3VzZXJfc3ViX3ZhbHVlIn0.FAKESIGNATUREFORTEST";

    #[test]
    fn test_extract_auth_metadata_jwt_with_padding_needed() {
        // The JWT payload is 1319 base64url chars (mod4=3), so decoding requires
        // URL_SAFE_NO_PAD — the engine must not add '=' padding characters.
        let auth_json: Value = serde_json::json!({
            "auth_mode": "chatgpt",
            "tokens": {
                "id_token": SYNTHETIC_ID_TOKEN
            }
        });
        let meta = extract_auth_metadata(&auth_json);
        assert_eq!(
            meta.email.as_deref(),
            Some("testuser@example.com"),
            "email must be extracted from JWT id_token"
        );
        assert_eq!(
            meta.plan.as_deref(),
            Some("plus"),
            "plan must be extracted from JWT id_token"
        );
    }

    #[test]
    fn test_load_profile_metadata_explicit_false_preserved() {
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join("profile.json"),
            b"{\"id\":\"x\",\"enabled\":false}",
        )
        .unwrap();
        let result = load_profile_metadata(tmp.path()).unwrap();
        assert!(
            !result.enabled,
            "profile.json with enabled=false must be preserved"
        );
    }
}
