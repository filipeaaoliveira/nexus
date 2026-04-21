use std::path::PathBuf;

use crate::config::Config;

use super::generic_store::GenericProfileStore;
use super::storage_backend::ProviderBackend;
use nexus_domain::accounts::Provider;
use super::traits::ProfileStore;

/// Supplies ProfileStore implementations by provider type.
pub trait StoreProvider: Send + Sync {
    fn store_for(&self, provider: Provider) -> Box<dyn ProfileStore>;
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
    fn store_for(&self, provider: Provider) -> Box<dyn ProfileStore> {
        let profiles_dir = match provider {
            Provider::Codex => &self.codex_profiles_dir,
            Provider::Claude => &self.claude_profiles_dir,
        };
        Box::new(GenericProfileStore::new(ProviderBackend::new(
            provider,
            profiles_dir.clone(),
        )))
    }
}
