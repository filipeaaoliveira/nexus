use crate::domain::accounts::Provider;

pub type ProfileId = String;

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
    ProfilesLoaded,
    CodexQuotaRefreshed {
        profile_id: ProfileId,
    },
    ClaudeQuotaRefreshed {
        profile_id: ProfileId,
    },
    ProfileImported {
        provider: Provider,
        profile_id: ProfileId,
    },
    ProfileWrittenToHome {
        provider: Provider,
        profile_id: ProfileId,
    },
    ProfileDeleted {
        provider: Provider,
        profile_id: ProfileId,
    },
    Failed {
        effect: Effect,
        message: String,
    },
}
