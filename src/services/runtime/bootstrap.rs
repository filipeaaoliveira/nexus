use crate::app::RootState;
use crate::config::Config;
use crate::domain::accounts::Provider;
use crate::services::accounts::quota::load_quota_cache;

pub(super) fn load_root_state(config: &Config) -> RootState {
    let mut root = RootState::default();

    if let Ok(cache) = load_quota_cache(&config.codex_quota_cache_file) {
        for (profile_id, quota) in cache.by_profile {
            root.load_cached_quota(Provider::Codex, profile_id, quota);
        }
    }

    if let Ok(cache) = load_quota_cache(&config.claude_quota_cache_file) {
        for (profile_id, quota) in cache.by_profile {
            root.load_cached_quota(Provider::Claude, profile_id, quota);
        }
    }

    root
}
