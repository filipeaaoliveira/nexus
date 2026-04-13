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
    pub fn profile_id(&self) -> &str {
        self.id.as_str()
    }
}
