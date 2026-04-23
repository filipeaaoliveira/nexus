use crate::codex_profiles::ProfileAuth;
use chrono::Utc;
use reqwest::Client;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use tokio::sync::Mutex;

const OPENAI_TOKEN_URL: &str = "https://auth.openai.com/oauth/token";

type RefreshLock = Arc<Mutex<()>>;

static REFRESH_LOCKS: OnceLock<std::sync::Mutex<HashMap<PathBuf, RefreshLock>>> = OnceLock::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefreshMode {
    IfNeeded,
    Force,
}

pub async fn ensure_fresh_auth(
    auth_path: PathBuf,
    auth: ProfileAuth,
    mode: RefreshMode,
) -> Result<ProfileAuth, String> {
    ensure_fresh_auth_against(auth_path, auth, mode, OPENAI_TOKEN_URL).await
}

async fn ensure_fresh_auth_against(
    auth_path: PathBuf,
    auth: ProfileAuth,
    mode: RefreshMode,
    token_url: &str,
) -> Result<ProfileAuth, String> {
    if mode == RefreshMode::IfNeeded && !auth.should_refresh_access_token(Utc::now()) {
        return Ok(auth);
    }

    let lock = refresh_lock_for(&auth_path);
    let _guard = lock.lock().await;

    let original = auth.clone();
    let latest = load_auth_or_original(auth_path.clone(), auth).await?;
    if mode == RefreshMode::IfNeeded && !latest.should_refresh_access_token(Utc::now()) {
        return Ok(latest);
    }
    if mode == RefreshMode::Force
        && auth_material_changed(&latest, &original)
        && !latest.should_refresh_access_token(Utc::now())
    {
        return Ok(latest);
    }

    let refreshed = refresh_token_against(latest, token_url).await?;
    save_auth(auth_path, refreshed.clone()).await?;
    Ok(refreshed)
}

fn auth_material_changed(latest: &ProfileAuth, original: &ProfileAuth) -> bool {
    latest.access_token() != original.access_token()
        || latest.refresh_token() != original.refresh_token()
        || latest.account_id() != original.account_id()
        || latest
            .tokens
            .as_ref()
            .and_then(|tokens| tokens.id_token.as_deref())
            != original
                .tokens
                .as_ref()
                .and_then(|tokens| tokens.id_token.as_deref())
        || latest.last_refresh != original.last_refresh
}

fn refresh_lock_for(auth_path: &Path) -> RefreshLock {
    let locks = REFRESH_LOCKS.get_or_init(|| std::sync::Mutex::new(HashMap::new()));
    let mut locks = locks.lock().expect("refresh lock map poisoned");
    locks
        .entry(auth_path.to_path_buf())
        .or_insert_with(|| Arc::new(Mutex::new(())))
        .clone()
}

async fn load_auth_or_original(
    auth_path: PathBuf,
    original: ProfileAuth,
) -> Result<ProfileAuth, String> {
    match tokio::task::spawn_blocking(move || ProfileAuth::load(&auth_path)).await {
        Ok(Ok(auth)) => Ok(auth),
        Ok(Err(_)) => Ok(original),
        Err(error) => Err(format!("failed to join Codex auth reload task: {error}")),
    }
}

async fn save_auth(auth_path: PathBuf, auth: ProfileAuth) -> Result<(), String> {
    tokio::task::spawn_blocking(move || auth.save(&auth_path))
        .await
        .map_err(|e| format!("failed to join Codex auth save task: {e}"))?
        .map_err(|e| format!("failed to save refreshed Codex auth: {e}"))
}

#[derive(Debug, Deserialize)]
struct TokenRefreshResponse {
    access_token: Option<String>,
    refresh_token: Option<String>,
    id_token: Option<String>,
}

async fn refresh_token_against(auth: ProfileAuth, token_url: &str) -> Result<ProfileAuth, String> {
    let Some(refresh_token) = auth.refresh_token() else {
        return Err("missing refresh_token in auth".to_string());
    };
    let refresh_token = refresh_token.to_string();
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("token refresh client build failed: {e}"))?;

    let body = [
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh_token.as_str()),
        ("client_id", ProfileAuth::oauth_client_id()),
    ];

    let resp = client
        .post(token_url)
        .form(&body)
        .send()
        .await
        .map_err(|e| format!("token refresh request failed: {e}"))?;

    let status = resp.status();
    if !status.is_success() {
        let body = resp
            .text()
            .await
            .unwrap_or_else(|e| format!("failed to read response body: {e}"));
        return Err(format!(
            "token refresh failed: HTTP {status}: {}",
            body.chars().take(512).collect::<String>()
        ));
    }

    let refreshed = resp
        .json::<TokenRefreshResponse>()
        .await
        .map_err(|e| format!("token refresh response parse failed: {e}"))?;
    let Some(new_access) = refreshed.access_token else {
        return Err("token refresh response missing access_token".to_string());
    };

    let mut updated = auth;
    if let Some(ref mut tokens) = updated.tokens {
        tokens.access_token = Some(new_access);
        if let Some(new_refresh) = refreshed.refresh_token {
            tokens.refresh_token = Some(new_refresh);
        }
        if let Some(new_id) = refreshed.id_token {
            tokens.id_token = Some(new_id);
        }
    }
    updated.last_refresh = Some(Utc::now());
    Ok(updated)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codex_profiles::auth::Tokens;
    use chrono::{DateTime, Utc};
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use tempfile::TempDir;

    fn auth(
        last_refresh: Option<DateTime<Utc>>,
        access_token: &str,
        refresh_token: &str,
    ) -> ProfileAuth {
        ProfileAuth {
            auth_mode: "chatgpt".to_string(),
            tokens: Some(Tokens {
                access_token: Some(access_token.to_string()),
                refresh_token: Some(refresh_token.to_string()),
                id_token: Some("old-id".to_string()),
                account_id: Some("account-123".to_string()),
            }),
            account_id: None,
            openai_api_key: None,
            last_refresh,
        }
    }

    fn spawn_token_server(
        status_line: &'static str,
        body: &'static str,
    ) -> (String, std::thread::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0_u8; 8192];
            let read = stream.read(&mut buf).unwrap();
            let request = String::from_utf8_lossy(&buf[..read]).into_owned();
            let response = format!(
                "{}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                status_line,
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).unwrap();
            request
        });
        (url, handle)
    }

    #[tokio::test]
    async fn ensure_fresh_auth_refreshes_stale_auth_and_persists_it() {
        let temp = TempDir::new().unwrap();
        let auth_path = temp.path().join("auth.json");
        let stale = auth(
            Some(Utc::now() - chrono::Duration::days(30)),
            "old-access",
            "old-refresh",
        );
        stale.save(&auth_path).unwrap();
        let (url, server) = spawn_token_server(
            "HTTP/1.1 200 OK",
            r#"{"access_token":"new-access","refresh_token":"new-refresh","id_token":"new-id"}"#,
        );

        let refreshed =
            ensure_fresh_auth_against(auth_path.clone(), stale, RefreshMode::IfNeeded, &url)
                .await
                .expect("stale auth should refresh");

        let request = server.join().unwrap();
        assert!(
            request.contains("content-type: application/x-www-form-urlencoded")
                || request.contains("Content-Type: application/x-www-form-urlencoded"),
            "{request}"
        );
        assert!(request.contains("grant_type=refresh_token"), "{request}");
        assert!(request.contains("refresh_token=old-refresh"), "{request}");
        assert!(
            request.contains(&format!("client_id={}", ProfileAuth::oauth_client_id())),
            "{request}"
        );
        assert_eq!(refreshed.access_token(), Some("new-access"));
        assert_eq!(refreshed.refresh_token(), Some("new-refresh"));

        let saved = ProfileAuth::load(&auth_path).unwrap();
        assert_eq!(saved.access_token(), Some("new-access"));
        assert_eq!(saved.refresh_token(), Some("new-refresh"));
    }

    #[tokio::test]
    async fn ensure_fresh_auth_reloads_inside_lock_and_skips_refresh_when_another_task_saved_fresh_auth(
    ) {
        let temp = TempDir::new().unwrap();
        let auth_path = temp.path().join("auth.json");
        let stale = auth(None, "old-access", "old-refresh");
        let fresh = auth(Some(Utc::now()), "fresh-access", "fresh-refresh");
        fresh.save(&auth_path).unwrap();

        let refreshed = ensure_fresh_auth_against(
            auth_path,
            stale,
            RefreshMode::IfNeeded,
            "http://127.0.0.1:9",
        )
        .await
        .expect("fresh reloaded auth should be reused");

        assert_eq!(refreshed.access_token(), Some("fresh-access"));
        assert_eq!(refreshed.refresh_token(), Some("fresh-refresh"));
    }

    #[tokio::test]
    async fn force_refresh_reuses_reloaded_auth_when_another_task_already_saved_new_tokens() {
        let temp = TempDir::new().unwrap();
        let auth_path = temp.path().join("auth.json");
        let rejected = auth(Some(Utc::now()), "rejected-access", "rejected-refresh");
        let fresh = auth(Some(Utc::now()), "fresh-access", "fresh-refresh");
        fresh.save(&auth_path).unwrap();

        let refreshed = ensure_fresh_auth_against(
            auth_path,
            rejected,
            RefreshMode::Force,
            "http://127.0.0.1:9",
        )
        .await
        .expect("changed reloaded auth should be reused");

        assert_eq!(refreshed.access_token(), Some("fresh-access"));
        assert_eq!(refreshed.refresh_token(), Some("fresh-refresh"));
    }

    #[tokio::test]
    async fn ensure_fresh_auth_reports_refresh_failure_without_saving_old_auth_as_fresh() {
        let temp = TempDir::new().unwrap();
        let auth_path = temp.path().join("auth.json");
        let stale = auth(None, "old-access", "old-refresh");
        stale.save(&auth_path).unwrap();
        let (url, server) = spawn_token_server(
            "HTTP/1.1 400 Bad Request",
            r#"{"error":"invalid_grant","error_description":"Refresh token already used"}"#,
        );

        let err = ensure_fresh_auth_against(auth_path.clone(), stale, RefreshMode::IfNeeded, &url)
            .await
            .expect_err("refresh failure should be reported");

        let _request = server.join().unwrap();
        assert!(
            err.contains("token refresh failed: HTTP 400 Bad Request"),
            "{err}"
        );
        assert!(err.contains("invalid_grant"), "{err}");
        let saved = ProfileAuth::load(&auth_path).unwrap();
        assert_eq!(saved.access_token(), Some("old-access"));
        assert_eq!(saved.refresh_token(), Some("old-refresh"));
    }
}
