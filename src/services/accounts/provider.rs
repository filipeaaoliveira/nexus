use std::path::PathBuf;

use crate::config::Config;

use super::generic_store::GenericProfileStore;
use super::storage_backend::{ClaudeStorageBackend, CodexStorageBackend};
use super::traits::ProfileStore;

/// Supplies ProfileStore implementations by provider type.
pub trait StoreProvider: Send + Sync {
    fn codex_store(&self) -> Box<dyn ProfileStore>;
    fn claude_store(&self) -> Box<dyn ProfileStore>;
}

#[derive(Clone)]
pub struct ConfigStoreProvider {
    codex_profiles_dir: PathBuf,
    claude_profiles_dir: PathBuf,
}

impl ConfigStoreProvider {
    pub fn from_config(config: &Config) -> Self {
        Self {
            codex_profiles_dir: config.codex_profiles_dir.clone(),
            claude_profiles_dir: config.claude_profiles_dir.clone(),
        }
    }
}

impl StoreProvider for ConfigStoreProvider {
    fn codex_store(&self) -> Box<dyn ProfileStore> {
        Box::new(GenericProfileStore::new(CodexStorageBackend {
            profiles_dir: self.codex_profiles_dir.clone(),
        }))
    }

    fn claude_store(&self) -> Box<dyn ProfileStore> {
        Box::new(GenericProfileStore::new(ClaudeStorageBackend {
            profiles_dir: self.claude_profiles_dir.clone(),
        }))
    }
}
