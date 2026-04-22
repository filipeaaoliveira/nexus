pub mod id;
pub mod model;
pub mod profile;
pub mod provider;
pub mod quota;

pub use id::{ProfileId, ProfileIdError};
pub use model::AccountEntry;
pub use profile::DomainProfile;
pub use provider::{Provider, ProviderFilter};
pub use quota::{Confidence, CreditsState, LimitState, QuotaCache, QuotaSnapshot, QuotaWindow};

/// A plain value object representing editable fields of an account profile.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EditableProfile {
    pub label: String,
    pub notes: String,
    pub enabled: bool,
}
