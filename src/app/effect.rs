use crate::domain::accounts::{AccountEntry, ClaudeProfile, CodexProfile, Provider, QuotaSnapshot};

pub type ProfileId = String;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccountProfile {
    Codex(CodexProfile),
    Claude(ClaudeProfile),
}

impl AccountProfile {
    pub fn provider(&self) -> Provider {
        match self {
            Self::Codex(_) => Provider::Codex,
            Self::Claude(_) => Provider::Claude,
        }
    }

    pub fn profile_id(&self) -> &str {
        match self {
            Self::Codex(profile) => &profile.id,
            Self::Claude(profile) => &profile.id,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    LoadProfiles,
    RefreshCodexQuota {
        profile_id: ProfileId,
    },
    RefreshClaudeQuota {
        profile_id: ProfileId,
    },
    ImportCodexProfile,
    ImportClaudeProfile,
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
        profiles: Vec<AccountProfile>,
        entries: Vec<AccountEntry>,
    },
    CodexQuotaRefreshed {
        profile_id: ProfileId,
        quota: QuotaSnapshot,
    },
    ClaudeQuotaRefreshed {
        profile_id: ProfileId,
        quota: QuotaSnapshot,
    },
    ProfileImported {
        profile: AccountProfile,
    },
    ProfileWrittenToHome {
        entry: AccountEntry,
    },
    ProfileDeleted {
        entry: AccountEntry,
    },
    Failed {
        effect: Effect,
        message: String,
    },
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use std::path::PathBuf;

    use crate::domain::accounts::{
        AccountEntry, ClaudeProfile, CodexProfile, Confidence, LimitState, Provider, QuotaSnapshot,
    };

    use super::*;

    fn codex_profile(id: &str) -> CodexProfile {
        let now = Utc::now();
        CodexProfile {
            id: id.to_string(),
            label: format!("Codex {id}"),
            email: Some(format!("{id}@example.com")),
            plan: Some("pro".to_string()),
            notes: Some("notes".to_string()),
            enabled: true,
            last_used: None,
            created_at: now,
            updated_at: now,
            profile_dir: PathBuf::from(format!("/tmp/{id}")),
            auth_path: PathBuf::from(format!("/tmp/{id}/auth.json")),
        }
    }

    fn claude_profile(id: &str) -> ClaudeProfile {
        let now = Utc::now();
        ClaudeProfile {
            id: id.to_string(),
            label: format!("Claude {id}"),
            email: Some(format!("{id}@example.com")),
            plan: Some("max".to_string()),
            notes: Some("notes".to_string()),
            enabled: true,
            last_used: None,
            created_at: now,
            updated_at: now,
            profile_dir: PathBuf::from(format!("/tmp/{id}")),
            auth_path: PathBuf::from(format!("/tmp/{id}/.credentials.json")),
        }
    }

    fn quota_snapshot(summary: &str) -> QuotaSnapshot {
        QuotaSnapshot::legacy_fallback(
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
                AccountProfile::Codex(codex_profile("codex-main")),
                AccountProfile::Claude(claude_profile("claude-main")),
            ],
            entries: vec![
                AccountEntry {
                    provider: Provider::Codex,
                    profile_id: "codex-main".to_string(),
                },
                AccountEntry {
                    provider: Provider::Claude,
                    profile_id: "claude-main".to_string(),
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
        let result = EffectResult::CodexQuotaRefreshed {
            profile_id: "codex-main".to_string(),
            quota: quota_snapshot("fresh snapshot"),
        };

        match result {
            EffectResult::CodexQuotaRefreshed { profile_id, quota } => {
                assert_eq!(profile_id, "codex-main");
                assert_eq!(quota.summary, "fresh snapshot");
            }
            other => panic!("unexpected result: {other:?}"),
        }
    }

    #[test]
    fn account_profile_exposes_provider_and_profile_id_for_state_keying() {
        let profile = AccountProfile::Claude(claude_profile("claude-main"));

        assert_eq!(profile.provider(), Provider::Claude);
        assert_eq!(profile.profile_id(), "claude-main");
    }

    #[test]
    fn import_and_delete_results_carry_reducer_usable_data() {
        let imported = EffectResult::ProfileImported {
            profile: AccountProfile::Claude(claude_profile("claude-main")),
        };
        let deleted = EffectResult::ProfileDeleted {
            entry: AccountEntry {
                provider: Provider::Claude,
                profile_id: "claude-main".to_string(),
            },
        };

        match imported {
            EffectResult::ProfileImported {
                profile: AccountProfile::Claude(profile),
            } => assert_eq!(profile.id, "claude-main"),
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
}
