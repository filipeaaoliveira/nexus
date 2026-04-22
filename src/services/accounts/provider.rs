use std::path::PathBuf;

use crate::config::Config;

use super::storage_backend::ProviderBackend;
use super::traits::ProfileStore;
use nexus_domain::accounts::Provider;

/// Supplies ProfileStore implementations by provider type.
pub trait StoreProvider: Send + Sync {
    type Store: ProfileStore;
    fn store_for(&self, provider: Provider) -> Self::Store;
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
    type Store = ProviderBackend;

    fn store_for(&self, provider: Provider) -> Self::Store {
        let profiles_dir = match provider {
            Provider::Codex => &self.codex_profiles_dir,
            Provider::Claude => &self.claude_profiles_dir,
            _ => unreachable!(),
        };
        ProviderBackend::new(provider, profiles_dir.clone())
    }
}
