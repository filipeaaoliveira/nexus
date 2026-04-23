use crate::app::dispatch::effects::dispatch_effect;
use crate::app::{Effect, EffectResult, RefreshOrigin};
use crate::config::Config;
use crate::services::accounts::provider::{ConfigStoreProvider, StoreProvider};
use nexus_domain::accounts::ProfileId;

use super::RuntimeSession;

pub(super) async fn refresh_quota(
    config: &Config,
    stores: &(impl StoreProvider + ?Sized),
    provider: nexus_domain::accounts::Provider,
    profile_id: String,
    origin: RefreshOrigin,
) -> EffectResult {
    let profile_id = ProfileId::new_unchecked(profile_id);
    dispatch_effect(
        config,
        stores,
        Effect::RefreshQuota {
            provider,
            profile_id,
            origin,
        },
    )
    .await
}

pub(super) fn spawn_manual_quota_refresh(
    session: &mut RuntimeSession,
    effect_tx: &tokio::sync::mpsc::Sender<EffectResult>,
) {
    let Some((provider, profile_id)) = session.selected_account() else {
        return;
    };
    let profile_id = profile_id.to_string();
    session.begin_manual_quota_refresh(provider, &profile_id);
    let tx = effect_tx.clone();
    let config = session.config_arc();
    let stores = ConfigStoreProvider::from_config(&config);
    tokio::spawn(async move {
        let result = refresh_quota(
            &config,
            &stores,
            provider,
            profile_id,
            RefreshOrigin::Manual,
        )
        .await;
        let _ = tx.send(result).await;
    });
}

pub(super) async fn poll_quota_loop<F, Fut>(
    profiles_dir: std::path::PathBuf,
    tx: tokio::sync::mpsc::Sender<EffectResult>,
    fetch_profile: F,
    token: tokio_util::sync::CancellationToken,
) where
    F: Fn(String, std::path::PathBuf) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = EffectResult> + Send + 'static,
{
    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(30));

    loop {
        tokio::select! {
            biased;

            _ = token.cancelled() => break,
            _ = interval.tick() => {
                refresh_all_profiles_once(&profiles_dir, &tx, &fetch_profile).await;
            }
        }
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
    use nexus_domain::accounts::{Confidence, LimitState, ProfileId, QuotaSnapshot};
    use tokio_util::sync::CancellationToken;

    fn test_snapshot() -> QuotaSnapshot {
        QuotaSnapshot::from_legacy(
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

        let token = CancellationToken::new();
        let (tx, mut rx) = mpsc::channel(4);
        let handle = tokio::spawn(poll_quota_loop(
            temp.path().to_path_buf(),
            tx,
            |pid, _| async move {
                EffectResult::QuotaRefreshed {
                    provider: nexus_domain::accounts::Provider::Codex,
                    profile_id: ProfileId::new_unchecked(pid),
                    quota: test_snapshot(),
                    origin: RefreshOrigin::Background,
                }
            },
            token.clone(),
        ));

        let result = tokio::time::timeout(Duration::from_millis(100), rx.recv())
            .await
            .expect("quota refresh should happen immediately")
            .expect("quota message");

        match result {
            EffectResult::QuotaRefreshed {
                provider,
                profile_id,
                quota,
                origin,
            } => {
                assert_eq!(provider, nexus_domain::accounts::Provider::Codex);
                assert_eq!(profile_id, "profile-01");
                assert_eq!(quota.summary, test_snapshot().summary);
                assert_eq!(origin, RefreshOrigin::Background);
            }
            other => panic!("unexpected result: {other:?}"),
        }

        token.cancel();
        handle.await.expect("task should exit gracefully");
    }

    #[tokio::test]
    async fn poll_quota_loop_forwards_failed_refreshes() {
        let temp = tempdir().expect("tempdir");
        let profile_dir = temp.path().join("profile-01");
        fs::create_dir_all(&profile_dir).expect("profile dir");

        let token = CancellationToken::new();
        let (tx, mut rx) = mpsc::channel(4);
        let handle = tokio::spawn(poll_quota_loop(
            temp.path().to_path_buf(),
            tx,
            |pid, _| async move {
                EffectResult::Failed {
                    effect: Effect::RefreshQuota {
                        provider: nexus_domain::accounts::Provider::Codex,
                        profile_id: ProfileId::new_unchecked(pid),
                        origin: RefreshOrigin::Background,
                    },
                    error: crate::app::effect::AppError::QuotaRefresh(
                        crate::app::effect::QuotaRefreshError::ApiFailed {
                            detail: "failed refresh".to_string(),
                        },
                    ),
                }
            },
            token.clone(),
        ));

        let result = tokio::time::timeout(Duration::from_millis(100), rx.recv())
            .await
            .expect("failed background refresh should still be forwarded")
            .expect("quota message");

        match result {
            EffectResult::Failed {
                effect:
                    Effect::RefreshQuota {
                        provider,
                        profile_id,
                        origin,
                    },
                ..
            } => {
                assert_eq!(provider, nexus_domain::accounts::Provider::Codex);
                assert_eq!(profile_id, "profile-01");
                assert_eq!(origin, RefreshOrigin::Background);
            }
            other => panic!("unexpected result: {other:?}"),
        }

        token.cancel();
        handle.await.expect("task should exit gracefully");
    }

    #[tokio::test]
    async fn poll_quota_loop_exits_promptly_on_cancellation() {
        let temp = tempdir().expect("tempdir");
        let profile_dir = temp.path().join("profile-01");
        fs::create_dir_all(&profile_dir).expect("profile dir");

        let token = CancellationToken::new();
        let (tx, mut rx) = mpsc::channel(4);
        let handle = tokio::spawn(poll_quota_loop(
            temp.path().to_path_buf(),
            tx,
            |pid, _| async move {
                EffectResult::QuotaRefreshed {
                    provider: nexus_domain::accounts::Provider::Codex,
                    profile_id: ProfileId::new_unchecked(pid),
                    quota: test_snapshot(),
                    origin: RefreshOrigin::Background,
                }
            },
            token.clone(),
        ));

        // Wait for first refresh to confirm the loop is running.
        rx.recv().await.expect("first refresh");

        token.cancel();
        tokio::time::timeout(Duration::from_secs(1), handle)
            .await
            .expect("task should complete within 1 second")
            .expect("task should exit without panic");
    }
}
