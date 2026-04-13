use crate::app::dispatch::effects::dispatch_effect;
use crate::app::effect::ProfileId;
use crate::app::{Effect, EffectResult, RefreshOrigin};
use crate::config::Config;

use super::RuntimeSession;

pub(super) async fn refresh_codex_quota(
    config: &Config,
    profile_id: String,
    origin: RefreshOrigin,
) -> EffectResult {
    let profile_id: ProfileId = profile_id.into();
    dispatch_effect(config, Effect::RefreshCodexQuota { profile_id, origin }).await
}

pub(super) async fn refresh_claude_quota(
    config: &Config,
    profile_id: String,
    origin: RefreshOrigin,
) -> EffectResult {
    let profile_id: ProfileId = profile_id.into();
    dispatch_effect(config, Effect::RefreshClaudeQuota { profile_id, origin }).await
}

pub(super) fn spawn_manual_quota_refresh(
    session: &mut RuntimeSession,
    effect_tx: &tokio::sync::mpsc::Sender<EffectResult>,
) {
    match session.selected_account() {
        Some((crate::domain::accounts::Provider::Claude, profile_id)) => {
            let profile_id = profile_id.to_string();
            session
                .begin_manual_quota_refresh(crate::domain::accounts::Provider::Claude, &profile_id);
            let tx = effect_tx.clone();
            let config = session.config().clone();
            tokio::spawn(async move {
                let result = refresh_claude_quota(&config, profile_id, RefreshOrigin::Manual).await;
                let _ = tx.send(result).await;
            });
        }
        Some((crate::domain::accounts::Provider::Codex, profile_id)) => {
            let profile_id = profile_id.to_string();
            session
                .begin_manual_quota_refresh(crate::domain::accounts::Provider::Codex, &profile_id);
            let tx = effect_tx.clone();
            let config = session.config().clone();
            tokio::spawn(async move {
                let result = refresh_codex_quota(&config, profile_id, RefreshOrigin::Manual).await;
                let _ = tx.send(result).await;
            });
        }
        None => {}
    }
}

pub(super) async fn poll_quota_loop<F, Fut>(
    profiles_dir: std::path::PathBuf,
    tx: tokio::sync::mpsc::Sender<EffectResult>,
    fetch_profile: F,
) where
    F: Fn(String, std::path::PathBuf) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = EffectResult> + Send + 'static,
{
    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(30));

    loop {
        interval.tick().await;
        refresh_all_profiles_once(&profiles_dir, &tx, &fetch_profile).await;
    }
}

async fn refresh_all_profiles_once<F, Fut>(
    profiles_dir: &std::path::Path,
    tx: &tokio::sync::mpsc::Sender<EffectResult>,
    fetch_profile: &F,
) where
    F: Fn(String, std::path::PathBuf) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = EffectResult> + Send + 'static,
{
    let profiles: Vec<String> = match std::fs::read_dir(profiles_dir) {
        Ok(entries) => entries
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_dir())
            .filter_map(|e| e.file_name().into_string().ok())
            .collect(),
        Err(_) => return,
    };

    let mut set = tokio::task::JoinSet::new();
    for profile_id in profiles {
        let profile_dir = profiles_dir.join(&profile_id);
        set.spawn(fetch_profile(profile_id, profile_dir));
    }

    while let Some(Ok(result)) = set.join_next().await {
        let _ = tx.send(result).await;
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::Duration;

    use chrono::Utc;
    use tempfile::tempdir;
    use tokio::sync::mpsc;

    use super::poll_quota_loop;
    use crate::app::{Effect, EffectResult, RefreshOrigin};
    use crate::codex_profiles::quota::{Confidence, LimitState, QuotaSnapshot};

    fn test_snapshot() -> QuotaSnapshot {
        QuotaSnapshot::legacy_fallback(
            Utc::now(),
            Confidence::Exact,
            LimitState::Available,
            None,
            Some(25),
            LimitState::Available,
            None,
            Some(40),
            "5h=25%; weekly=40%".to_string(),
        )
    }

    #[tokio::test]
    async fn poll_quota_loop_refreshes_immediately_before_first_interval() {
        let temp = tempdir().expect("tempdir");
        let profile_dir = temp.path().join("profile-01");
        fs::create_dir_all(&profile_dir).expect("profile dir");

        let (tx, mut rx) = mpsc::channel(4);
        let handle = tokio::spawn(poll_quota_loop(
            temp.path().to_path_buf(),
            tx,
            |pid, _| async move {
                EffectResult::CodexQuotaRefreshed {
                    profile_id: pid.into(),
                    quota: test_snapshot(),
                    origin: RefreshOrigin::Background,
                }
            },
        ));

        let result = tokio::time::timeout(Duration::from_millis(100), rx.recv())
            .await
            .expect("quota refresh should happen immediately")
            .expect("quota message");

        match result {
            EffectResult::CodexQuotaRefreshed {
                profile_id,
                quota,
                origin,
            } => {
                assert_eq!(profile_id, "profile-01");
                assert_eq!(quota.summary, test_snapshot().summary);
                assert_eq!(origin, RefreshOrigin::Background);
            }
            other => panic!("unexpected result: {other:?}"),
        }

        handle.abort();
    }

    #[tokio::test]
    async fn poll_quota_loop_forwards_failed_refreshes() {
        let temp = tempdir().expect("tempdir");
        let profile_dir = temp.path().join("profile-01");
        fs::create_dir_all(&profile_dir).expect("profile dir");

        let (tx, mut rx) = mpsc::channel(4);
        let handle = tokio::spawn(poll_quota_loop(
            temp.path().to_path_buf(),
            tx,
            |pid, _| async move {
                EffectResult::Failed {
                    effect: Effect::RefreshCodexQuota {
                        profile_id: pid.into(),
                        origin: RefreshOrigin::Background,
                    },
                    message: "failed refresh".to_string(),
                }
            },
        ));

        let result = tokio::time::timeout(Duration::from_millis(100), rx.recv())
            .await
            .expect("failed background refresh should still be forwarded")
            .expect("quota message");

        match result {
            EffectResult::Failed {
                effect: Effect::RefreshCodexQuota { profile_id, origin },
                ..
            } => {
                assert_eq!(profile_id, "profile-01");
                assert_eq!(origin, RefreshOrigin::Background);
            }
            other => panic!("unexpected result: {other:?}"),
        }

        handle.abort();
    }
}
