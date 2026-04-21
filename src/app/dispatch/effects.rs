use crate::app::effect::{
    AppError, Effect, EffectResult, QuotaRefreshError, RefreshOrigin, StorageError,
};
use crate::app::state::EditableProfile;
use crate::claude_profiles::auth::ClaudeAuth;
use crate::claude_profiles::quota::{fetch_claude_quota, refresh_token_if_needed};
use crate::codex_profiles::quota::fetch_quota_from_api;
use crate::codex_profiles::ProfileAuth;
use crate::config::Config;
use crate::services::accounts::provider::StoreProvider;
use crate::services::accounts::traits::ProfileStore;
use nexus_domain::accounts::{AccountEntry, DomainProfile, Provider};

pub async fn dispatch_effect(
    config: &Config,
    stores: &dyn StoreProvider,
    effect: Effect,
) -> EffectResult {
    match dispatch_effect_inner(config, stores, &effect).await {
        Ok(result) => result,
        Err(error) => EffectResult::Failed { effect, error },
    }
}

async fn dispatch_effect_inner(
    config: &Config,
    stores: &dyn StoreProvider,
    effect: &Effect,
) -> Result<EffectResult, AppError> {
    match effect {
        Effect::LoadProfiles => load_profiles(stores.codex_store(), stores.claude_store()).await,
        Effect::LoadEditableProfile {
            provider,
            profile_id,
        } => match provider {
            Provider::Codex => load_profile_draft(stores.codex_store(), profile_id).await,
            Provider::Claude => load_profile_draft(stores.claude_store(), profile_id).await,
        },
        Effect::RefreshQuota {
            provider,
            profile_id,
            origin,
        } => match provider {
            Provider::Codex => refresh_codex_quota(config, profile_id, *origin).await,
            Provider::Claude => refresh_claude_quota(config, profile_id, *origin).await,
        },
        Effect::ImportProfile { provider } => match provider {
            Provider::Codex => import_profile(stores.codex_store()).await,
            Provider::Claude => import_profile(stores.claude_store()).await,
        },
        Effect::SaveProfileEdits {
            provider,
            profile_id,
            draft,
        } => match provider {
            Provider::Codex => save_profile_edits(stores.codex_store(), profile_id, draft).await,
            Provider::Claude => save_profile_edits(stores.claude_store(), profile_id, draft).await,
        },
        Effect::WriteProfileToHome {
            provider,
            profile_id,
        } => match provider {
            Provider::Codex => write_profile_to_home(stores.codex_store(), profile_id).await,
            Provider::Claude => write_profile_to_home(stores.claude_store(), profile_id).await,
        },
        Effect::DeleteProfile {
            provider,
            profile_id,
        } => match provider {
            Provider::Codex => delete_profile(stores.codex_store(), profile_id).await,
            Provider::Claude => delete_profile(stores.claude_store(), profile_id).await,
        },
    }
}

fn storage_err(error: impl std::fmt::Display) -> AppError {
    AppError::Storage(StorageError::Io {
        detail: error.to_string(),
    })
}

fn store_to_app_error(error: crate::services::accounts::storage::ProfileStoreError) -> AppError {
    use crate::services::accounts::storage::ProfileStoreError;
    match error {
        ProfileStoreError::NotFound { profile_id } => AppError::Storage(StorageError::NotFound { path: profile_id }),
        ProfileStoreError::Corrupted { path, detail } => AppError::Storage(StorageError::CorruptedData { path, detail }),
        ProfileStoreError::Io(e) => AppError::Storage(StorageError::Io { detail: e.to_string() }),
        ProfileStoreError::Other { context, source } => AppError::Storage(StorageError::Io { detail: format!("{context}: {source}") }),
    }
}

fn quota_auth_missing(profile_id: &str) -> AppError {
    AppError::QuotaRefresh(QuotaRefreshError::AuthMissing {
        profile_id: profile_id.to_string(),
    })
}

async fn load_profiles(
    codex: Box<dyn ProfileStore>,
    claude: Box<dyn ProfileStore>,
) -> Result<EffectResult, AppError> {
    let (profiles, entries) = tokio::task::spawn_blocking(move || {
        let mut profiles = Vec::new();
        let mut entries = Vec::new();

        load_all_from_store(codex.as_ref(), &mut profiles, &mut entries);
        load_all_from_store(claude.as_ref(), &mut profiles, &mut entries);

        (profiles, entries)
    })
    .await
    .map_err(storage_err)?;

    Ok(EffectResult::ProfilesLoaded { profiles, entries })
}

fn load_all_from_store(
    store: &dyn ProfileStore,
    profiles: &mut Vec<DomainProfile>,
    entries: &mut Vec<AccountEntry>,
) {
    let provider = store.provider();
    for profile_id in store.scan_profile_ids() {
        match store.load_profile(&profile_id) {
            Ok(outcome) => {
                entries.push(AccountEntry {
                    provider,
                    profile_id: profile_id.clone().into(),
                });
                profiles.push(outcome.into_value());
            }
            Err(error) => {
                tracing::warn!(
                    profile_id = %profile_id,
                    provider = ?provider,
                    error = %error,
                    "skipping invalid profile during startup load"
                );
            }
        }
    }
}

async fn load_profile_draft(
    store: Box<dyn ProfileStore>,
    profile_id: &str,
) -> Result<EffectResult, AppError> {
    let profile_id = profile_id.to_string();
    let provider = store.provider();
    let pid = profile_id.clone();
    let draft = tokio::task::spawn_blocking(move || store.load_editable(&pid))
        .await
        .map_err(storage_err)?
        .map_err(store_to_app_error)?;

    Ok(EffectResult::EditableProfileLoaded {
        provider,
        profile_id: profile_id.into(),
        draft,
    })
}

async fn refresh_codex_quota(
    config: &Config,
    profile_id: &str,
    origin: RefreshOrigin,
) -> Result<EffectResult, AppError> {
    let auth_path = config.codex_profiles_dir.join(profile_id).join("auth.json");
    let auth = tokio::task::spawn_blocking(move || ProfileAuth::load(&auth_path))
        .await
        .map_err(storage_err)?
        .map_err(|_| quota_auth_missing(profile_id))?;
    let quota = fetch_quota_from_api(&auth)
        .await
        .map_err(|detail| AppError::QuotaRefresh(QuotaRefreshError::ApiFailed { detail }))?;
    Ok(EffectResult::QuotaRefreshed {
        provider: Provider::Codex,
        profile_id: profile_id.into(),
        quota,
        origin,
    })
}

async fn refresh_claude_quota(
    config: &Config,
    profile_id: &str,
    origin: RefreshOrigin,
) -> Result<EffectResult, AppError> {
    let creds_path = config
        .claude_profiles_dir
        .join(profile_id)
        .join(".credentials.json");
    let auth = tokio::task::spawn_blocking(move || ClaudeAuth::load(&creds_path))
        .await
        .map_err(storage_err)?
        .map_err(|_| quota_auth_missing(profile_id))?;
    let refreshed = refresh_token_if_needed(auth).await;
    let save_path = config
        .claude_profiles_dir
        .join(profile_id)
        .join(".credentials.json");
    let refreshed = persist_refreshed_claude_auth_best_effort(save_path, refreshed).await;
    let quota = fetch_claude_quota(&refreshed)
        .await
        .map_err(|detail| AppError::QuotaRefresh(QuotaRefreshError::ApiFailed { detail }))?;
    Ok(EffectResult::QuotaRefreshed {
        provider: Provider::Claude,
        profile_id: profile_id.into(),
        quota,
        origin,
    })
}

async fn import_profile(store: Box<dyn ProfileStore>) -> Result<EffectResult, AppError> {
    let home = dirs::home_dir().ok_or_else(|| storage_err("home directory not found"))?;
    let profile = tokio::task::spawn_blocking(move || (*store).import_from_home(&home))
        .await
        .map_err(storage_err)?
        .map_err(store_to_app_error)?;
    Ok(EffectResult::ProfileImported { profile })
}

async fn save_profile_edits(
    store: Box<dyn ProfileStore>,
    profile_id: &str,
    draft: &EditableProfile,
) -> Result<EffectResult, AppError> {
    let profile_id = profile_id.to_string();
    let draft = draft.clone();
    let pid = profile_id.clone();
    let profile = tokio::task::spawn_blocking(move || store.save_edits(&pid, &draft))
        .await
        .map_err(storage_err)?
        .map_err(store_to_app_error)?;

    Ok(EffectResult::ProfileSaved { profile })
}

async fn write_profile_to_home(
    store: Box<dyn ProfileStore>,
    profile_id: &str,
) -> Result<EffectResult, AppError> {
    let profile_id = profile_id.to_string();
    let provider = store.provider();
    let home = dirs::home_dir().ok_or_else(|| storage_err("home directory not found"))?;
    let pid = profile_id.clone();
    tokio::task::spawn_blocking(move || store.write_to_home(&pid, &home))
        .await
        .map_err(storage_err)?
        .map_err(store_to_app_error)?;

    Ok(EffectResult::ProfileWrittenToHome {
        entry: AccountEntry {
            provider,
            profile_id: profile_id.into(),
        },
    })
}

async fn delete_profile(
    store: Box<dyn ProfileStore>,
    profile_id: &str,
) -> Result<EffectResult, AppError> {
    let profile_id = profile_id.to_string();
    let provider = store.provider();
    let pid = profile_id.clone();
    tokio::task::spawn_blocking(move || store.delete(&pid))
        .await
        .map_err(storage_err)?
        .map_err(store_to_app_error)?;
    Ok(EffectResult::ProfileDeleted {
        entry: AccountEntry {
            provider,
            profile_id: profile_id.into(),
        },
    })
}

async fn persist_refreshed_claude_auth_best_effort(
    save_path: std::path::PathBuf,
    refreshed: ClaudeAuth,
) -> ClaudeAuth {
    let to_save = refreshed.clone();
    let path_for_log = save_path.clone();
    match tokio::task::spawn_blocking(move || to_save.save(&save_path)).await {
        Ok(Ok(())) => {}
        Ok(Err(error)) => {
            tracing::warn!(
                path = %path_for_log.display(),
                error = %error,
                "failed to persist refreshed Claude credentials; continuing with in-memory token"
            );
        }
        Err(error) => {
            tracing::warn!(
                path = %path_for_log.display(),
                error = %error,
                "failed to join Claude credential persistence task; continuing with in-memory token"
            );
        }
    }
    refreshed
}

#[cfg(test)]
#[path = "effects_mock_test.rs"]
mod mock_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::claude_profiles::auth::{ClaudeAuth, ClaudeOAuthTokens};
    use crate::config::Config;
    use crate::services::accounts::provider::ConfigStoreProvider;
    use std::path::Path;
    use tempfile::tempdir;

    fn test_config(root: &Path) -> Config {
        let mut config = Config::default();
        config.codex_management_dir = root.join("codex-management");
        config.codex_profiles_dir = config.codex_management_dir.join("profiles");
        config.claude_management_dir = root.join("claude-management");
        config.claude_profiles_dir = config.claude_management_dir.join("profiles");
        config.codex_quota_cache_file = config.codex_management_dir.join("quota-cache.json");
        config.claude_quota_cache_file = config.claude_management_dir.join("quota-cache.json");
        config
    }

    #[tokio::test]
    async fn dispatch_save_profile_edits_returns_profile_saved() {
        let temp = tempdir().unwrap();
        let config = test_config(temp.path());
        let profile_dir = config.codex_profiles_dir.join("profile-01");
        std::fs::create_dir_all(&profile_dir).unwrap();
        std::fs::write(
            profile_dir.join("auth.json"),
            r#"{"email":"dev@example.com","plan":"pro"}"#,
        )
        .unwrap();

        let stores = ConfigStoreProvider::from_config(&config);
        let result = dispatch_effect(
            &config,
            &stores,
            Effect::SaveProfileEdits {
                provider: Provider::Codex,
                profile_id: "profile-01".into(),
                draft: EditableProfile {
                    label: "Renamed".to_string(),
                    notes: "saved".to_string(),
                    enabled: false,
                },
            },
        )
        .await;

        match result {
            EffectResult::ProfileSaved { profile } => {
                assert_eq!(profile.provider, Provider::Codex);
                assert_eq!(profile.label, "Renamed");
                assert_eq!(profile.notes.as_deref(), Some("saved"));
                assert!(!profile.enabled);
            }
            other => panic!("unexpected result: {other:?}"),
        }
    }

    #[tokio::test]
    async fn dispatch_load_profile_draft_returns_editable_profile_fields() {
        let temp = tempdir().unwrap();
        let config = test_config(temp.path());
        let profile_dir = config.codex_profiles_dir.join("profile-03");
        std::fs::create_dir_all(&profile_dir).unwrap();
        std::fs::write(
            profile_dir.join("auth.json"),
            r#"{"email":"draft@example.com","plan":"team"}"#,
        )
        .unwrap();
        crate::services::accounts::storage::save_profile_edits(
            &profile_dir,
            "profile-03",
            "Draft Label",
            Some("editable".to_string()),
            false,
            false,
        )
        .unwrap();

        let stores = ConfigStoreProvider::from_config(&config);
        let result = dispatch_effect(
            &config,
            &stores,
            Effect::LoadEditableProfile {
                provider: Provider::Codex,
                profile_id: "profile-03".into(),
            },
        )
        .await;

        match result {
            EffectResult::EditableProfileLoaded {
                provider,
                profile_id,
                draft,
            } => {
                assert_eq!(provider, Provider::Codex);
                assert_eq!(profile_id, "profile-03");
                assert_eq!(draft.label, "Draft Label");
                assert_eq!(draft.notes, "editable");
                assert!(!draft.enabled);
            }
            other => panic!("unexpected result: {other:?}"),
        }
    }

    #[tokio::test]
    async fn dispatch_delete_profile_returns_deleted_entry() {
        let temp = tempdir().unwrap();
        let config = test_config(temp.path());
        let profile_dir = config.claude_profiles_dir.join("profile-02");
        std::fs::create_dir_all(&profile_dir).unwrap();
        std::fs::write(profile_dir.join(".credentials.json"), r#"{"claudeAiOauth":{"accessToken":"tok","refreshToken":"ref","expiresAt":9999999999000}}"#).unwrap();

        let stores = ConfigStoreProvider::from_config(&config);
        let result = dispatch_effect(
            &config,
            &stores,
            Effect::DeleteProfile {
                provider: Provider::Claude,
                profile_id: "profile-02".into(),
            },
        )
        .await;

        match result {
            EffectResult::ProfileDeleted { entry } => {
                assert_eq!(entry.provider, Provider::Claude);
                assert_eq!(entry.profile_id, "profile-02");
            }
            other => panic!("unexpected result: {other:?}"),
        }
    }

    #[tokio::test]
    async fn dispatch_refresh_claude_quota_missing_credentials_returns_failed() {
        let temp = tempdir().unwrap();
        let config = test_config(temp.path());

        let stores = ConfigStoreProvider::from_config(&config);
        let result = dispatch_effect(
            &config,
            &stores,
            Effect::RefreshQuota {
                provider: Provider::Claude,
                profile_id: "profile-09".into(),
                origin: RefreshOrigin::Manual,
            },
        )
        .await;

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
                assert_eq!(provider, Provider::Claude);
                assert_eq!(profile_id, "profile-09");
                assert_eq!(origin, RefreshOrigin::Manual);
            }
            other => panic!("unexpected result: {other:?}"),
        }
    }

    #[test]
    fn persist_refreshed_claude_auth_best_effort_swallows_write_failures() {
        let temp = tempdir().unwrap();
        let blocking_parent = temp.path().join("blocking-parent");
        std::fs::write(&blocking_parent, "not a directory").unwrap();
        let save_path = blocking_parent.join(".credentials.json");
        let auth = ClaudeAuth {
            claude_ai_oauth: Some(ClaudeOAuthTokens {
                access_token: "tok".to_string(),
                refresh_token: "ref".to_string(),
                expires_at: 9_999_999_999_000,
                scopes: vec![],
                subscription_type: Some("claude_max".to_string()),
            }),
        };

        let runtime = tokio::runtime::Runtime::new().unwrap();
        let persisted = runtime.block_on(persist_refreshed_claude_auth_best_effort(
            save_path,
            auth.clone(),
        ));

        assert_eq!(persisted.access_token(), auth.access_token());
        assert_eq!(persisted.refresh_token(), auth.refresh_token());
        assert_eq!(persisted.subscription_type(), auth.subscription_type());
    }
}
