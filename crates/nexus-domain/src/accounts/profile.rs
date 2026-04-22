use chrono::{DateTime, Utc};

use super::{ProfileId, Provider};

/// Domain-owned profile representation.
///
/// Captures the business-relevant fields shared across all providers.
/// Filesystem-specific details (`profile_dir`, `auth_path`) are intentionally
/// excluded -- those belong to the persistence layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DomainProfile {
    pub id: ProfileId,
    pub provider: Provider,
    pub label: String,
    pub email: Option<String>,
    pub plan: Option<String>,
    pub notes: Option<String>,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl DomainProfile {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: ProfileId,
        provider: Provider,
        label: String,
        email: Option<String>,
        plan: Option<String>,
        notes: Option<String>,
        enabled: bool,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
    ) -> Self {
        Self { id, provider, label, email, plan, notes, enabled, created_at, updated_at }
    }

    pub fn profile_id(&self) -> &str {
        self.id.as_str()
    }
}
