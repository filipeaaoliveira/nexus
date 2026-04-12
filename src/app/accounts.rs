use super::*;
use crate::app::dispatch::effects::dispatch_effect;
use crate::app::state::AccountKey;
use crate::services::accounts::quota as quota_service;
use std::time::{Duration, Instant};

impl AppState {
    pub fn begin_manual_quota_refresh(&mut self, provider: Provider, profile_id: &str) {
        self.root.accounts.quota_loading = true;
        self.interactive_quota_refreshes
            .insert(AccountKey::new(provider, profile_id.to_string()));
    }

    fn accounts_root_state(&self) -> RootState {
        self.root.clone()
    }

    fn apply_accounts_root_state(&mut self, root: &RootState) {
        self.root.accounts = root.accounts.clone();
        match &root.accounts.status_message {
            Some(_) => {
                self.accounts_status_message_expires_at =
                    Some(Instant::now() + Duration::from_secs(5));
            }
            None => {
                self.accounts_status_message_expires_at = None;
            }
        }
    }

    pub fn apply_accounts_effect_result(&mut self, result: EffectResult) {
        let refreshed_quota = match &result {
            EffectResult::CodexQuotaRefreshed {
                profile_id, quota, ..
            } => Some((Provider::Codex, profile_id.clone(), quota.clone())),
            EffectResult::ClaudeQuotaRefreshed {
                profile_id, quota, ..
            } => Some((Provider::Claude, profile_id.clone(), quota.clone())),
            _ => None,
        };
        let refresh_state = match &result {
            EffectResult::CodexQuotaRefreshed {
                profile_id, origin, ..
            } => Some((
                AccountKey::new(Provider::Codex, profile_id.clone()),
                *origin,
            )),
            EffectResult::ClaudeQuotaRefreshed {
                profile_id, origin, ..
            } => Some((
                AccountKey::new(Provider::Claude, profile_id.clone()),
                *origin,
            )),
            EffectResult::Failed {
                effect: Effect::RefreshCodexQuota { profile_id, origin },
                ..
            } => Some((
                AccountKey::new(Provider::Codex, profile_id.clone()),
                *origin,
            )),
            EffectResult::Failed {
                effect: Effect::RefreshClaudeQuota { profile_id, origin },
                ..
            } => Some((
                AccountKey::new(Provider::Claude, profile_id.clone()),
                *origin,
            )),
            _ => None,
        };
        let background_refresh_failure = matches!(
            &result,
            EffectResult::Failed {
                effect: Effect::RefreshCodexQuota { .. } | Effect::RefreshClaudeQuota { .. },
                ..
            }
        ) && matches!(
            refresh_state,
            Some((_, crate::app::RefreshOrigin::Background))
        );

        if background_refresh_failure {
            return;
        }

        if let Some((key, origin)) = &refresh_state {
            if matches!(origin, crate::app::RefreshOrigin::Manual) {
                self.interactive_quota_refreshes.remove(key);
                self.root.accounts.quota_loading = !self.interactive_quota_refreshes.is_empty();
            }
        }
        let mut root = self.accounts_root_state();
        reducer::root::reduce(&mut root, &Action::EffectResult(result));
        root.accounts.quota_loading = !self.interactive_quota_refreshes.is_empty();
        self.apply_accounts_root_state(&root);
        if let Some((provider, profile_id, quota)) = refreshed_quota {
            self.persist_refreshed_quota_cache(provider, &profile_id, quota);
        }
    }

    pub async fn run_accounts_reducer_action(&mut self, action: AccountsAction) {
        let mut root = self.accounts_root_state();
        let effects = reducer::root::reduce(&mut root, &Action::Accounts(action));
        self.apply_accounts_root_state(&root);

        for effect in effects {
            let result = dispatch_effect(&self.config, effect).await;
            self.apply_accounts_effect_result(result);
        }
    }

    pub async fn run_accounts_effect(&mut self, effect: Effect) {
        let result = dispatch_effect(&self.config, effect).await;
        self.apply_accounts_effect_result(result);
    }

    pub async fn initialize_accounts(&mut self) {
        self.run_accounts_effect(Effect::LoadProfiles).await;
    }

    pub fn dismiss_accounts_overlay(&mut self) {
        self.root.clear_overlay();
        self.root.accounts.mode = AccountsMode::Normal;
        let root = self.root.clone();
        self.apply_accounts_root_state(&root);
    }

    pub fn selected_codex_profile_id(&self) -> Option<&str> {
        match self.selected_account()? {
            (Provider::Codex, id) => Some(id),
            _ => None,
        }
    }

    pub fn selected_claude_profile_id(&self) -> Option<&str> {
        match self.selected_account()? {
            (Provider::Claude, id) => Some(id),
            _ => None,
        }
    }

    pub fn selected_account(&self) -> Option<(Provider, &str)> {
        self.root
            .accounts
            .selected_entry()
            .map(|entry| (entry.provider, entry.profile_id.as_str()))
    }

    pub fn set_active_codex_profile(&mut self, profile_id: Option<String>) {
        self.root.accounts.active_codex_profile_id = profile_id;
    }

    pub fn set_active_claude_profile(&mut self, profile_id: Option<String>) {
        self.root.accounts.active_claude_profile_id = profile_id;
    }

    pub fn active_codex_profile_id(&self) -> Option<&str> {
        self.root.accounts.active_codex_profile_id.as_deref()
    }

    fn persist_refreshed_quota_cache(
        &mut self,
        provider: Provider,
        profile_id: &str,
        quota: QuotaSnapshot,
    ) {
        let (path, label) = match provider {
            Provider::Codex => (&self.config.codex_quota_cache_file, "codex"),
            Provider::Claude => (&self.config.claude_quota_cache_file, "claude"),
        };
        let cache = Some(quota_cache_for_provider(&self.root.accounts.quota_by_key, provider));

        if let Err(error) = quota_service::persist_quota_snapshot(cache, profile_id, quota, path) {
            tracing::warn!("failed to save {label} quota cache: {error}");
        }
    }

}

fn quota_cache_for_provider(
    quota_by_key: &std::collections::BTreeMap<AccountKey, QuotaSnapshot>,
    provider: Provider,
) -> QuotaCache {
    let mut cache = QuotaCache::default();
    for (key, quota) in quota_by_key {
        if key.provider == provider {
            cache.by_profile.insert(key.profile_id.clone(), quota.clone());
        }
    }
    cache
}
