use nexus_domain::accounts::{AccountEntry, DomainProfile, ProfileId, Provider, QuotaSnapshot};

use super::state::EditableProfile;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefreshOrigin {
    Manual,
    Background,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StorageError {
    NotFound {
        path: String,
    },
    Io {
        detail: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuotaRefreshError {
    AuthMissing {
        profile_id: String,
    },
    ApiFailed {
        detail: String,
    },
    #[allow(dead_code)]
    ParseFailed {
        detail: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppError {
    Storage(StorageError),
    QuotaRefresh(QuotaRefreshError),
}

impl std::fmt::Display for StorageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound { path } => write!(f, "not found: {path}"),
            Self::Io { detail } => write!(f, "I/O error: {detail}"),
        }
    }
}

impl std::fmt::Display for QuotaRefreshError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AuthMissing { profile_id } => write!(f, "auth missing for {profile_id}"),
            Self::ApiFailed { detail } => write!(f, "API request failed: {detail}"),
            Self::ParseFailed { detail } => write!(f, "quota parse failed: {detail}"),
        }
    }
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Storage(e) => write!(f, "{e}"),
            Self::QuotaRefresh(e) => write!(f, "{e}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    LoadProfiles,
    LoadEditableProfile {
        provider: Provider,
        profile_id: ProfileId,
    },
    RefreshQuota {
        provider: Provider,
        profile_id: ProfileId,
        origin: RefreshOrigin,
    },
    ImportProfile {
        provider: Provider,
    },
    SaveProfileEdits {
        provider: Provider,
        profile_id: ProfileId,
        draft: EditableProfile,
    },
    WriteProfileToHome {
        provider: Provider,
        profile_id: ProfileId,
    },
    DeleteProfile {
        provider: Provider,
        profile_id: ProfileId,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EffectResult {
    ProfilesLoaded {
        profiles: Vec<DomainProfile>,
        entries: Vec<AccountEntry>,
    },
    EditableProfileLoaded {
        provider: Provider,
        profile_id: ProfileId,
        draft: EditableProfile,
    },
    QuotaRefreshed {
        provider: Provider,
        profile_id: ProfileId,
        quota: QuotaSnapshot,
        origin: RefreshOrigin,
    },
    ProfileImported {
        profile: DomainProfile,
    },
    ProfileSaved {
        profile: DomainProfile,
    },
    ProfileWrittenToHome {
        entry: AccountEntry,
    },
    ProfileDeleted {
        entry: AccountEntry,
    },
    Failed {
        effect: Effect,
        error: AppError,
    },
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use crate::profiles::StoredProfile;
    use nexus_domain::accounts::{
        AccountEntry, Confidence, LimitState, Provider, QuotaSnapshot,
    };

    use super::*;

    fn codex_profile(id: &str) -> StoredProfile {
        let now = Utc::now();
        StoredProfile {
            id: id.to_string(),
            label: format!("Codex {id}"),
            email: Some(format!("{id}@example.com")),
            plan: Some("pro".to_string()),
            notes: Some("notes".to_string()),
            enabled: true,
            last_used: None,
            created_at: now,
            updated_at: now,
        }
    }

    fn claude_profile(id: &str) -> StoredProfile {
        let now = Utc::now();
        StoredProfile {
            id: id.to_string(),
            label: format!("Claude {id}"),
            email: Some(format!("{id}@example.com")),
            plan: Some("max".to_string()),
            notes: Some("notes".to_string()),
            enabled: true,
            last_used: None,
            created_at: now,
            updated_at: now,
        }
    }

    fn quota_snapshot(summary: &str) -> QuotaSnapshot {
        QuotaSnapshot::from_legacy(
            Utc::now(),
            Confidence::Exact,
            LimitState::Available,
            None,
            Some(42),
            LimitState::Available,
            None,
            Some(84),
            summary.to_string(),
        )
    }

    #[test]
    fn profiles_loaded_carries_profile_models_and_entries() {
        let result = EffectResult::ProfilesLoaded {
            profiles: vec![
                codex_profile("codex-main").to_domain(Provider::Codex),
                claude_profile("claude-main").to_domain(Provider::Claude),
            ],
            entries: vec![
                AccountEntry {
                    provider: Provider::Codex,
                    profile_id: "codex-main".into(),
                },
                AccountEntry {
                    provider: Provider::Claude,
                    profile_id: "claude-main".into(),
                },
            ],
        };

        match result {
            EffectResult::ProfilesLoaded { profiles, entries } => {
                assert_eq!(profiles.len(), 2);
                assert_eq!(entries.len(), 2);
            }
            other => panic!("unexpected result: {other:?}"),
        }
    }

    #[test]
    fn quota_refresh_results_carry_refreshed_snapshot() {
        let result = EffectResult::QuotaRefreshed {
            provider: Provider::Codex,
            profile_id: "codex-main".into(),
            quota: quota_snapshot("fresh snapshot"),
            origin: RefreshOrigin::Manual,
        };

        match result {
            EffectResult::QuotaRefreshed {
                provider,
                profile_id,
                quota,
                origin,
            } => {
                assert_eq!(provider, Provider::Codex);
                assert_eq!(profile_id, "codex-main");
                assert_eq!(quota.summary, "fresh snapshot");
                assert_eq!(origin, RefreshOrigin::Manual);
            }
            other => panic!("unexpected result: {other:?}"),
        }
    }

    #[test]
    fn domain_profile_exposes_provider_and_profile_id_for_state_keying() {
        let profile = claude_profile("claude-main").to_domain(Provider::Claude);

        assert_eq!(profile.provider, Provider::Claude);
        assert_eq!(profile.profile_id(), "claude-main");
    }

    #[test]
    fn domain_profile_from_provider_profiles_preserves_provider() {
        let codex = codex_profile("codex-main").to_domain(Provider::Codex);
        let claude = claude_profile("claude-main").to_domain(Provider::Claude);

        assert_eq!(codex.provider, Provider::Codex);
        assert_eq!(claude.provider, Provider::Claude);
    }

    #[test]
    fn import_and_delete_results_carry_reducer_usable_data() {
        let imported = EffectResult::ProfileImported {
            profile: claude_profile("claude-main").to_domain(Provider::Claude),
        };
        let deleted = EffectResult::ProfileDeleted {
            entry: AccountEntry {
                provider: Provider::Claude,
                profile_id: "claude-main".into(),
            },
        };

        match imported {
            EffectResult::ProfileImported { profile } => {
                assert_eq!(profile.profile_id(), "claude-main");
                assert_eq!(profile.provider, Provider::Claude);
            }
            other => panic!("unexpected import result: {other:?}"),
        }

        match deleted {
            EffectResult::ProfileDeleted { entry } => {
                assert_eq!(entry.provider, Provider::Claude);
                assert_eq!(entry.profile_id, "claude-main");
            }
            other => panic!("unexpected delete result: {other:?}"),
        }
    }

    #[test]
    fn profile_draft_loaded_carries_selected_profile_identity_and_draft() {
        let result = EffectResult::EditableProfileLoaded {
            provider: Provider::Codex,
            profile_id: "codex-main".into(),
            draft: EditableProfile {
                label: "Main".to_string(),
                notes: "notes".to_string(),
                enabled: true,
            },
        };

        match result {
            EffectResult::EditableProfileLoaded {
                provider,
                profile_id,
                draft,
            } => {
                assert_eq!(provider, Provider::Codex);
                assert_eq!(profile_id, "codex-main");
                assert_eq!(draft.label, "Main");
            }
            other => panic!("unexpected draft result: {other:?}"),
        }
    }
}
