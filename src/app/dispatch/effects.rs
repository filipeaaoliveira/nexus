use crate::app::effect::{AccountProfile, Effect, EffectResult, RefreshOrigin};
use crate::app::state::ProfileDraft;
use crate::claude_profiles::auth::ClaudeAuth;
use crate::claude_profiles::quota::{fetch_claude_quota, refresh_token_if_needed};
use crate::codex_profiles::quota::fetch_quota_from_api;
use crate::codex_profiles::ProfileAuth;
use crate::config::Config;
use crate::domain::accounts::{AccountEntry, Provider};
use crate::services::accounts::{
    claude as claude_service, codex as codex_service, storage as storage_service,
};

pub async fn dispatch_effect(config: &Config, effect: Effect) -> EffectResult {
    match dispatch_effect_inner(config, &effect).await {
        Ok(result) => result,
        Err(error) => EffectResult::Failed {
            effect,
            message: error.to_string(),
        },
    }
}

async fn dispatch_effect_inner(config: &Config, effect: &Effect) -> anyhow::Result<EffectResult> {
    match effect {
        Effect::LoadProfiles => load_profiles(config).await,
        Effect::LoadProfileDraft {
            provider,
            profile_id,
        } => load_profile_draft(config, *provider, profile_id).await,
        Effect::RefreshCodexQuota { profile_id, origin } => {
            refresh_codex_quota(config, profile_id, *origin).await
        }
        Effect::RefreshClaudeQuota { profile_id, origin } => {
            refresh_claude_quota(config, profile_id, *origin).await
        }
        Effect::ImportCodexProfile => import_codex_profile(config).await,
        Effect::ImportClaudeProfile => import_claude_profile(config).await,
        Effect::SaveProfileEdits {
            provider,
            profile_id,
            draft,
        } => save_profile_edits(config, *provider, profile_id, draft).await,
        Effect::WriteProfileToHome {
            provider,
            profile_id,
        } => write_profile_to_home(config, *provider, profile_id).await,
        Effect::DeleteProfile {
            provider,
            profile_id,
        } => delete_profile(config, *provider, profile_id).await,
    }
}

async fn load_profiles(config: &Config) -> anyhow::Result<EffectResult> {
    let codex_profiles_dir = config.codex_profiles_dir.clone();
    let claude_profiles_dir = config.claude_profiles_dir.clone();
    let (profiles, entries) = tokio::task::spawn_blocking(move || {
        let mut profiles = Vec::new();
        let mut entries = Vec::new();

        for profile_id in storage_service::scan_codex_profile_ids(&codex_profiles_dir) {
            let profile_dir = codex_profiles_dir.join(&profile_id);
            match storage_service::load_codex_profile(&profile_dir) {
                Ok(profile) => {
                    entries.push(AccountEntry {
                        provider: Provider::Codex,
                        profile_id: profile_id.clone(),
                    });
                    profiles.push(AccountProfile::Codex(profile));
                }
                Err(error) => {
                    tracing::warn!(
                        profile_id = %profile_id,
                        path = %profile_dir.display(),
                        error = %error,
                        "skipping invalid Codex profile during startup load"
                    );
                }
            }
        }

        for profile_id in storage_service::scan_claude_profile_ids(&claude_profiles_dir) {
            let profile_dir = claude_profiles_dir.join(&profile_id);
            match storage_service::load_claude_profile(&profile_dir) {
                Ok(profile) => {
                    entries.push(AccountEntry {
                        provider: Provider::Claude,
                        profile_id: profile_id.clone(),
                    });
                    profiles.push(AccountProfile::Claude(profile));
                }
                Err(error) => {
                    tracing::warn!(
                        profile_id = %profile_id,
                        path = %profile_dir.display(),
                        error = %error,
                        "skipping invalid Claude profile during startup load"
                    );
                }
            }
        }

        anyhow::Ok((profiles, entries))
    })
    .await??;

    Ok(EffectResult::ProfilesLoaded { profiles, entries })
}

async fn load_profile_draft(
    config: &Config,
    provider: Provider,
    profile_id: &str,
) -> anyhow::Result<EffectResult> {
    let profile_id = profile_id.to_string();
    let draft = match provider {
        Provider::Codex => {
            let profile_dir = config.codex_profiles_dir.join(&profile_id);
            tokio::task::spawn_blocking(move || {
                storage_service::load_codex_profile(&profile_dir).map(|profile| ProfileDraft {
                    label: profile.label,
                    email: profile.email,
                    plan: profile.plan,
                    notes: profile.notes.unwrap_or_default(),
                    enabled: profile.enabled,
                })
            })
            .await??
        }
        Provider::Claude => {
            let profile_dir = config.claude_profiles_dir.join(&profile_id);
            tokio::task::spawn_blocking(move || {
                storage_service::load_claude_profile(&profile_dir).map(|profile| ProfileDraft {
                    label: profile.label,
                    email: profile.email,
                    plan: profile.plan,
                    notes: profile.notes.unwrap_or_default(),
                    enabled: profile.enabled,
                })
            })
            .await??
        }
    };

    Ok(EffectResult::ProfileDraftLoaded {
        provider,
        profile_id,
        draft,
    })
}

async fn refresh_codex_quota(
    config: &Config,
    profile_id: &str,
    origin: RefreshOrigin,
) -> anyhow::Result<EffectResult> {
    let auth_path = config.codex_profiles_dir.join(profile_id).join("auth.json");
    let auth = tokio::task::spawn_blocking(move || ProfileAuth::load(&auth_path)).await??;
    let quota = fetch_quota_from_api(&auth)
        .await
        .ok_or_else(|| anyhow::anyhow!("failed to refresh Codex quota"))?;
    Ok(EffectResult::CodexQuotaRefreshed {
        profile_id: profile_id.to_string(),
        quota,
        origin,
    })
}

async fn refresh_claude_quota(
    config: &Config,
    profile_id: &str,
    origin: RefreshOrigin,
) -> anyhow::Result<EffectResult> {
    let creds_path = config
        .claude_profiles_dir
        .join(profile_id)
        .join(".credentials.json");
    let auth = tokio::task::spawn_blocking(move || ClaudeAuth::load(&creds_path)).await??;
    let refreshed = refresh_token_if_needed(auth).await;
    let save_path = config
        .claude_profiles_dir
        .join(profile_id)
        .join(".credentials.json");
    let refreshed = persist_refreshed_claude_auth_best_effort(save_path, refreshed).await;
    let quota = fetch_claude_quota(&refreshed)
        .await
        .ok_or_else(|| anyhow::anyhow!("failed to refresh Claude quota"))?;
    Ok(EffectResult::ClaudeQuotaRefreshed {
        profile_id: profile_id.to_string(),
        quota,
        origin,
    })
}

async fn import_codex_profile(config: &Config) -> anyhow::Result<EffectResult> {
    let profiles_dir = config.codex_profiles_dir.clone();
    let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("home directory not found"))?;
    let profile = tokio::task::spawn_blocking(move || {
        codex_service::import_profile_from_home(&profiles_dir, &home)
    })
    .await??;
    Ok(EffectResult::ProfileImported {
        profile: AccountProfile::Codex(profile),
    })
}

async fn import_claude_profile(config: &Config) -> anyhow::Result<EffectResult> {
    let profiles_dir = config.claude_profiles_dir.clone();
    let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("home directory not found"))?;
    let profile = tokio::task::spawn_blocking(move || {
        claude_service::import_profile_from_home(&profiles_dir, &home)
    })
    .await??;
    Ok(EffectResult::ProfileImported {
        profile: AccountProfile::Claude(profile),
    })
}

async fn save_profile_edits(
    config: &Config,
    provider: Provider,
    profile_id: &str,
    draft: &ProfileDraft,
) -> anyhow::Result<EffectResult> {
    let profile_id = profile_id.to_string();
    let draft = draft.clone();
    let profile = match provider {
        Provider::Codex => {
            let profile_dir = config.codex_profiles_dir.join(&profile_id);
            tokio::task::spawn_blocking(move || {
                storage_service::save_codex_profile_metadata(
                    &profile_dir,
                    &profile_id,
                    &draft.label,
                    notes_option(&draft.notes),
                    draft.enabled,
                )?;
                storage_service::load_codex_profile(&profile_dir).map(AccountProfile::Codex)
            })
            .await??
        }
        Provider::Claude => {
            let profile_dir = config.claude_profiles_dir.join(&profile_id);
            tokio::task::spawn_blocking(move || {
                storage_service::save_claude_profile_edits(
                    &profile_dir,
                    &profile_id,
                    &draft.label,
                    notes_option(&draft.notes),
                    draft.enabled,
                )?;
                storage_service::load_claude_profile(&profile_dir).map(AccountProfile::Claude)
            })
            .await??
        }
    };

    Ok(EffectResult::ProfileSaved { profile })
}

async fn write_profile_to_home(
    config: &Config,
    provider: Provider,
    profile_id: &str,
) -> anyhow::Result<EffectResult> {
    let profile_id = profile_id.to_string();
    let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("home directory not found"))?;
    match provider {
        Provider::Codex => {
            let profile_dir = config.codex_profiles_dir.join(&profile_id);
            tokio::task::spawn_blocking(move || {
                codex_service::write_profile_to_home(&profile_dir, &home)
            })
            .await??;
        }
        Provider::Claude => {
            let profile_dir = config.claude_profiles_dir.join(&profile_id);
            tokio::task::spawn_blocking(move || {
                claude_service::write_profile_to_home(&profile_dir, &home)
            })
            .await??;
        }
    }

    Ok(EffectResult::ProfileWrittenToHome {
        entry: AccountEntry {
            provider,
            profile_id,
        },
    })
}

async fn delete_profile(
    config: &Config,
    provider: Provider,
    profile_id: &str,
) -> anyhow::Result<EffectResult> {
    let profile_id = profile_id.to_string();
    let profile_dir = match provider {
        Provider::Codex => config.codex_profiles_dir.join(&profile_id),
        Provider::Claude => config.claude_profiles_dir.join(&profile_id),
    };
    tokio::task::spawn_blocking(move || storage_service::delete_profile_dir(&profile_dir))
        .await??;
    Ok(EffectResult::ProfileDeleted {
        entry: AccountEntry {
            provider,
            profile_id,
        },
    })
}

fn notes_option(notes: &str) -> Option<String> {
    if notes.is_empty() {
        None
    } else {
        Some(notes.to_string())
    }
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
mod tests {
    use super::*;
    use crate::claude_profiles::auth::{ClaudeAuth, ClaudeOAuthTokens};
    use crate::config::Config;
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

        let result = dispatch_effect(
            &config,
            Effect::SaveProfileEdits {
                provider: Provider::Codex,
                profile_id: "profile-01".to_string(),
                draft: ProfileDraft {
                    label: "Renamed".to_string(),
                    email: Some("dev@example.com".to_string()),
                    plan: Some("pro".to_string()),
                    notes: "saved".to_string(),
                    enabled: false,
                },
            },
        )
        .await;

        match result {
            EffectResult::ProfileSaved {
                profile: AccountProfile::Codex(profile),
            } => {
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
        crate::services::accounts::storage::save_codex_profile_metadata(
            &profile_dir,
            "profile-03",
            "Draft Label",
            Some("editable".to_string()),
            false,
        )
        .unwrap();

        let result = dispatch_effect(
            &config,
            Effect::LoadProfileDraft {
                provider: Provider::Codex,
                profile_id: "profile-03".to_string(),
            },
        )
        .await;

        match result {
            EffectResult::ProfileDraftLoaded {
                provider,
                profile_id,
                draft,
            } => {
                assert_eq!(provider, Provider::Codex);
                assert_eq!(profile_id, "profile-03");
                assert_eq!(draft.label, "Draft Label");
                assert_eq!(draft.email.as_deref(), Some("draft@example.com"));
                assert_eq!(draft.plan.as_deref(), Some("team"));
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

        let result = dispatch_effect(
            &config,
            Effect::DeleteProfile {
                provider: Provider::Claude,
                profile_id: "profile-02".to_string(),
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

        let result = dispatch_effect(
            &config,
            Effect::RefreshClaudeQuota {
                profile_id: "profile-09".to_string(),
                origin: RefreshOrigin::Manual,
            },
        )
        .await;

        match result {
            EffectResult::Failed {
                effect: Effect::RefreshClaudeQuota { profile_id, origin },
                ..
            } => {
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
