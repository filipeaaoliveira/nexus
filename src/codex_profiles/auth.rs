use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileAuth {
    pub auth_mode: String,
    pub tokens: Option<Tokens>,
    pub account_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tokens {
    pub access_token: Option<String>,
    #[serde(rename = "account_id")]
    pub account_id: Option<String>,
}

impl ProfileAuth {
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        serde_json::from_str(&content)
            .with_context(|| format!("failed to parse {}", path.display()))
    }

    pub fn account_id(&self) -> Option<&str> {
        self.tokens
            .as_ref()
            .and_then(|t| t.account_id.as_deref())
            .or(self.account_id.as_deref())
    }

    pub fn access_token(&self) -> Option<&str> {
        self.tokens.as_ref().and_then(|t| t.access_token.as_deref())
    }
}
