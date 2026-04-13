use super::provider::Provider;
use crate::app::effect::ProfileId;

/// A profile entry in the unified account list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountEntry {
    pub provider: Provider,
    pub profile_id: ProfileId,
}
