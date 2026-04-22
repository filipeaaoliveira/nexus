use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use nexus_domain::accounts::{DomainProfile, ProfileId, Provider};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StoredProfile {
    pub id: String,
    pub label: String,
    pub email: Option<String>,
    pub plan: Option<String>,
    pub notes: Option<String>,
    pub enabled: bool,
    pub last_used: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl StoredProfile {
    pub fn to_domain(self, provider: Provider) -> DomainProfile {
        DomainProfile {
            id: ProfileId::new_unchecked(self.id),
            provider,
            label: self.label,
            email: self.email,
            plan: self.plan,
            notes: self.notes,
            enabled: self.enabled,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn to_domain_sets_provider() {
        let now = Utc::now();
        let stored = StoredProfile {
            id: "p1".into(),
            label: "Test".into(),
            email: Some("a@b.com".into()),
            plan: Some("pro".into()),
            notes: None,
            enabled: true,
            last_used: None,
            created_at: now,
            updated_at: now,
        };
        let domain = stored.to_domain(Provider::Codex);
        assert_eq!(domain.provider, Provider::Codex);
        assert_eq!(domain.profile_id(), "p1");
    }
}
