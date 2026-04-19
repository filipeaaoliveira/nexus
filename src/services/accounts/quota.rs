use std::path::Path;

use anyhow::Result;

use nexus_domain::accounts::{QuotaCache, QuotaSnapshot};

pub fn load_quota_cache(path: &Path) -> Result<QuotaCache> {
    nexus_domain::accounts::quota::load_quota_cache(path)
}

pub fn save_quota_cache(cache: &QuotaCache, path: &Path) -> Result<()> {
    nexus_domain::accounts::quota::save_quota_cache(cache, path)
}

pub fn persist_quota_snapshot(
    cache: Option<QuotaCache>,
    profile_id: &str,
    quota: QuotaSnapshot,
    path: &Path,
) -> Result<QuotaCache> {
    let mut cache = cache.unwrap_or_default();
    cache.by_profile.insert(profile_id.to_string(), quota);
    save_quota_cache(&cache, path)?;
    Ok(cache)
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use tempfile::tempdir;

    use crate::services::accounts::quota::{
        load_quota_cache, persist_quota_snapshot, save_quota_cache,
    };
    use nexus_domain::accounts::{Confidence, LimitState, QuotaCache, QuotaSnapshot};

    #[test]
    fn quota_cache_round_trips_snapshot_by_profile() {
        let temp = tempdir().unwrap();
        let cache_path = temp.path().join("quota-cache.json");
        let mut cache = QuotaCache::default();
        cache.by_profile.insert(
            "profile-01".to_string(),
            QuotaSnapshot::legacy_fallback(
                Utc::now(),
                Confidence::Exact,
                LimitState::Available,
                None,
                Some(10),
                LimitState::Hit,
                None,
                Some(100),
                "cached snapshot".to_string(),
            ),
        );

        save_quota_cache(&cache, &cache_path).unwrap();
        let loaded = load_quota_cache(&cache_path).unwrap();

        assert_eq!(
            loaded
                .by_profile
                .get("profile-01")
                .map(|snapshot| snapshot.summary.as_str()),
            Some("cached snapshot")
        );
    }

    #[test]
    fn persist_quota_snapshot_updates_cache_and_writes_file() {
        let temp = tempdir().unwrap();
        let cache_path = temp.path().join("quota-cache.json");
        let snapshot = QuotaSnapshot::legacy_fallback(
            Utc::now(),
            Confidence::Exact,
            LimitState::Available,
            None,
            Some(20),
            LimitState::Available,
            None,
            Some(200),
            "persisted snapshot".to_string(),
        );

        let cache = persist_quota_snapshot(None, "profile-02", snapshot, &cache_path).unwrap();
        let loaded = load_quota_cache(&cache_path).unwrap();

        assert_eq!(
            cache
                .by_profile
                .get("profile-02")
                .map(|quota| quota.summary.as_str()),
            Some("persisted snapshot")
        );
        assert_eq!(
            loaded
                .by_profile
                .get("profile-02")
                .map(|quota| quota.summary.as_str()),
            Some("persisted snapshot")
        );
    }
}
