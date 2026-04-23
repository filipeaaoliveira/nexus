use chrono::{DateTime, Utc};

use super::{ProfileId, Provider};

/// Domain-owned profile representation.
///
/// Captures the business-relevant fields shared across all providers.
/// Filesystem-specific details (`profile_dir`, `auth_path`) are intentionally
/// excluded -- those belong to the persistence layer.
///
/// All fields are private; use accessors to read and [`DomainProfile::new`] to
/// construct.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DomainProfile {
    id: ProfileId,
    provider: Provider,
    label: String,
    email: Option<String>,
    plan: Option<String>,
    notes: Option<String>,
    enabled: bool,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
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
        Self {
            id,
            provider,
            label,
            email,
            plan,
            notes,
            enabled,
            created_at,
            updated_at,
        }
    }

    pub fn id(&self) -> &ProfileId {
        &self.id
    }

    pub fn profile_id(&self) -> &str {
        self.id.as_str()
    }

    pub fn provider(&self) -> Provider {
        self.provider
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn email(&self) -> Option<&str> {
        self.email.as_deref()
    }

    pub fn plan(&self) -> Option<&str> {
        self.plan.as_deref()
    }

    pub fn notes(&self) -> Option<&str> {
        self.notes.as_deref()
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    pub fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
}
