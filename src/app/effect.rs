use crate::claude_profiles::profile::ClaudeProfile;
use crate::codex_profiles::profile::CodexProfile;
use crate::domain::accounts::{AccountEntry, Provider, QuotaSnapshot};

use super::state::ProfileDraft;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProfileId(String);

impl ProfileId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for ProfileId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<String> for ProfileId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for ProfileId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl AsRef<str> for ProfileId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl std::ops::Deref for ProfileId {
    type Target = str;
    fn deref(&self) -> &str {
        &self.0
    }
}

impl PartialEq<str> for ProfileId {
    fn eq(&self, other: &str) -> bool {
        self.0 == other
    }
}

impl PartialEq<&str> for ProfileId {
    fn eq(&self, other: &&str) -> bool {
        self.0 == *other
    }
}

impl PartialEq<String> for ProfileId {
    fn eq(&self, other: &String) -> bool {
        self.0 == *other
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefreshOrigin {
    Manual,
    Background,
}

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

impl From<CodexProfile> for AccountProfile {
    fn from(profile: CodexProfile) -> Self {
        Self::Codex(profile)
    }
}

impl From<ClaudeProfile> for AccountProfile {
    fn from(profile: ClaudeProfile) -> Self {
        Self::Claude(profile)
    }
}

/// Categorized error for effect failures.
///
/// Preserves structure so the reducer and runtime can branch on category
/// (e.g. suppress background refresh failures) while still providing a
/// human-readable message for UI display.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppError {
    /// Profile loading, importing, saving, or deletion failed.
    Storage { message: String },
    /// Quota refresh failed (network, auth, parsing).
    QuotaRefresh { message: String },
    /// Catch-all for failures that don't fit a specific category yet.
    #[allow(dead_code)]
    Other { message: String },
}

impl AppError {
    pub fn message(&self) -> &str {
        match self {
            Self::Storage { message } => message,
            Self::QuotaRefresh { message } => message,
            Self::Other { message } => message,
        }
    }
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.message())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    LoadProfiles,
    LoadProfileDraft {
        provider: Provider,
        profile_id: ProfileId,
    },
    RefreshCodexQuota {
        profile_id: ProfileId,
        origin: RefreshOrigin,
    },
    RefreshClaudeQuota {
        profile_id: ProfileId,
        origin: RefreshOrigin,
    },
    ImportCodexProfile,
    ImportClaudeProfile,
    SaveProfileEdits {
        provider: Provider,
        profile_id: ProfileId,
        draft: ProfileDraft,
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
        profiles: Vec<AccountProfile>,
        entries: Vec<AccountEntry>,
    },
    ProfileDraftLoaded {
        provider: Provider,
        profile_id: ProfileId,
        draft: ProfileDraft,
    },
    CodexQuotaRefreshed {
        profile_id: ProfileId,
        quota: QuotaSnapshot,
        origin: RefreshOrigin,
    },
    ClaudeQuotaRefreshed {
        profile_id: ProfileId,
        quota: QuotaSnapshot,
        origin: RefreshOrigin,
    },
    ProfileImported {
        profile: AccountProfile,
    },
    ProfileSaved {
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
        error: AppError,
    },
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use std::path::PathBuf;

    use crate::claude_profiles::profile::ClaudeProfile;
    use crate::codex_profiles::profile::CodexProfile;
    use crate::codex_profiles::quota::{Confidence, LimitState, QuotaSnapshot};
    use crate::domain::accounts::{AccountEntry, Provider};

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
            profile_id: "codex-main".into(),
            quota: quota_snapshot("fresh snapshot"),
            origin: RefreshOrigin::Manual,
        };

        match result {
            EffectResult::CodexQuotaRefreshed {
                profile_id,
                quota,
                origin,
            } => {
                assert_eq!(profile_id, "codex-main");
                assert_eq!(quota.summary, "fresh snapshot");
                assert_eq!(origin, RefreshOrigin::Manual);
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
    fn account_profile_from_profile_models_preserves_variant() {
        let codex = AccountProfile::from(codex_profile("codex-main"));
        let claude = AccountProfile::from(claude_profile("claude-main"));

        assert!(matches!(codex, AccountProfile::Codex(_)));
        assert!(matches!(claude, AccountProfile::Claude(_)));
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

    #[test]
    fn profile_draft_loaded_carries_selected_profile_identity_and_draft() {
        let result = EffectResult::ProfileDraftLoaded {
            provider: Provider::Codex,
            profile_id: "codex-main".into(),
            draft: ProfileDraft {
                label: "Main".to_string(),
                email: Some("main@example.com".to_string()),
                plan: Some("pro".to_string()),
                notes: "notes".to_string(),
                enabled: true,
            },
        };

        match result {
            EffectResult::ProfileDraftLoaded {
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
