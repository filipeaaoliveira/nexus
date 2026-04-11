use super::provider::Provider;

/// A profile entry in the unified account list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountEntry {
    pub provider: Provider,
    pub profile_id: String,
}

pub use crate::claude_profiles::profile::{ClaudeProfile, ClaudeProfileMetadata};
pub use crate::codex_profiles::profile::{AuthMetadata, CodexProfile, ProfileMetadata};
