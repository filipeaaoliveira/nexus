use anyhow::{bail, Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::path::Path;

use crate::profiles::StoredProfile;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct AuthMetadata {
    pub email: Option<String>,
    pub plan: Option<String>,
}

pub fn import_from_auth(
    id: &str,
    label: &str,
    profile_dir: &Path,
    auth_source: &Path,
) -> Result<StoredProfile> {
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

    fs::create_dir_all(profile_dir)
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

    Ok(StoredProfile {
        id: id.to_string(),
        label: label.to_string(),
        email: metadata.email,
        plan: metadata.plan,
        notes: None,
        enabled: true,
        last_used: None,
        created_at: now,
        updated_at: now,
    })
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

#[cfg(test)]
mod tests {
    use super::*;

    // Synthetic JWT token for testing JWT parsing — all fields are fake, no real user data.
    // Payload is 1319 base64url chars (mod4=3), which requires one '=' when padded.
    // URL_SAFE_NO_PAD must decode this correctly without adding padding characters.
    const SYNTHETIC_ID_TOKEN: &str = "eyJhbGciOiJSUzI1NiIsImtpZCI6IjAwMDAwMDAwLTAwMDAtMDAwMC0wMDAwLTAwMDAwMDAwMDAwMCIsInR5cCI6IkpXVCJ9.eyJhbXIiOlsib3RwIiwidXJuOm9wZW5haTphbXI6b3RwX2VtYWlsIl0sImF0X2hhc2giOiJBQUFBQUFBQUFBQUFBQUFBQUFBQUFBIiwiYXVkIjpbImFwcF9BQUFBQUFBQUFBQUFBQUFBQUFBQUFBIl0sImF1dGhfcHJvdmlkZXIiOiJwYXNzd29yZGxlc3MiLCJhdXRoX3RpbWUiOjE3MDAwMDAwMDAsImVtYWlsIjoidGVzdHVzZXJAZXhhbXBsZS5jb20iLCJlbWFpbF92ZXJpZmllZCI6dHJ1ZSwiZXhwIjoxNzAwMDAzNjAwLCJodHRwczovL2FwaS5vcGVuYWkuY29tL2F1dGgiOnsiY2hhdGdwdF9hY2NvdW50X2lkIjoiMDAwMDAwMDAtMDAwMC0wMDAwLTAwMDAtMDAwMDAwMDAwMDAxIiwiY2hhdGdwdF9wbGFuX3R5cGUiOiJwbHVzIiwiY2hhdGdwdF9zdWJzY3JpcHRpb25fYWN0aXZlX3N0YXJ0IjoiMjAyMy0xMS0wMVQwMDowMDowMCswMDowMCIsImNoYXRncHRfc3Vic2NyaXB0aW9uX2FjdGl2ZV91bnRpbCI6IjIwMjMtMTItMDFUMDA6MDA6MDArMDA6MDAiLCJjaGF0Z3B0X3N1YnNjcmlwdGlvbl9sYXN0X2NoZWNrZWQiOiIyMDIzLTExLTE1VDAwOjAwOjAwLjAwMDAwMCswMDowMCIsImNoYXRncHRfdXNlcl9pZCI6InVzZXItQUFBQUFBQUFBQUFBQUFBQUFBQUFBQSIsImdyb3VwcyI6W10sImxvY2FsaG9zdCI6ZmFsc2UsIm9yZ2FuaXphdGlvbnMiOlt7ImlkIjoib3JnLUFBQUFBQUFBQUFBQUFBQUFBQUFBQUEiLCJpc19kZWZhdWx0Ijp0cnVlLCJyb2xlIjoib3duZXIiLCJ0aXRsZSI6IlBlcnNvbmFsIn1dLCJ1c2VyX2lkIjoidXNlci1BQUFBQUFBQUFBQUFBQUFBQUFBQUFBIn0sImlhdCI6MTcwMDAwMDAwMCwiaXNzIjoiaHR0cHM6Ly9hdXRoLm9wZW5haS5jb20iLCJqdGkiOiIwMDAwMDAwMC0wMDAwLTAwMDAtMDAwMC0wMDAwMDAwMDAwMDIiLCJuYW1lIjoiVGVzdCBVc2VyIiwicmF0IjoxNjk5OTk5OTk5LCJzaWQiOiIwMDAwMDAwMC0wMDAwLTAwMDAtMDAwMC0wMDAwMDAwMDAwMDMiLCJzdWIiOiJhdXRoMHx0ZXN0X3VzZXJfc3ViX3ZhbHVlIn0.FAKESIGNATUREFORTEST";

    #[test]
    fn test_extract_auth_metadata_jwt_with_padding_needed() {
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
}
