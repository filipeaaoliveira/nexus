use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileMetadata {
    pub id: String,
    pub label: Option<String>,
    pub email: Option<String>,
    pub plan: Option<String>,
    pub notes: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

impl Default for ProfileMetadata {
    fn default() -> Self {
        Self {
            id: String::new(),
            label: None,
            email: None,
            plan: None,
            notes: None,
            enabled: true,
            created_at: None,
            updated_at: None,
        }
    }
}
