use std::collections::BTreeSet;
use std::ops::{Deref, DerefMut};
use std::time::{Duration, Instant};

use crate::app::dispatch::effects::dispatch_effect;
use crate::app::state::AccountKey;
use crate::app::{reducer, AccountsAction, Action, AppState, Effect, EffectResult, Provider};
use crate::config::Config;
use crate::services::accounts::provider::ConfigStoreProvider;
use crate::services::accounts::quota as quota_service;
use nexus_domain::accounts::{QuotaCache, QuotaSnapshot};

pub(crate) struct RuntimeSession {
    app: AppState,
    config: Config,
    stores: ConfigStoreProvider,
    interactive_quota_refreshes: BTreeSet<AccountKey>,
    accounts_status_message_expires_at: Option<Instant>,
    started_at: Instant,
}

impl RuntimeSession {
    pub(crate) fn new(config: Config, app: AppState) -> Self {
        let stores = ConfigStoreProvider::from_config(&config);
        Self {
            app,
            config,
            stores,
            interactive_quota_refreshes: BTreeSet::new(),
            accounts_status_message_expires_at: None,
            started_at: Instant::now(),
        }
    }

    pub(crate) fn app(&self) -> &AppState {
        &self.app
    }

    pub(crate) fn config(&self) -> &Config {
        &self.config
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn config_mut(&mut self) -> &mut Config {
        &mut self.config
    }

    pub(crate) fn started_at(&self) -> Instant {
        self.started_at
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn set_started_at(&mut self, started_at: Instant) {
        self.started_at = started_at;
    }

    pub(crate) fn begin_manual_quota_refresh(&mut self, provider: Provider, profile_id: &str) {
        let mut root = self.app.accounts_root_state();
        root.set_accounts_quota_loading(true);
        self.app.apply_accounts_root_state(&root);
        self.interactive_quota_refreshes
            .insert(AccountKey::new(provider, profile_id.to_string()));
    }

    pub(crate) fn clear_expired_accounts_status(&mut self) {
        if let Some(expires_at) = self.accounts_status_message_expires_at {
            if Instant::now() >= expires_at {
                let mut root = self.app.accounts_root_state();
                root.clear_accounts_status_message();
                self.app.apply_accounts_root_state(&root);
                self.accounts_status_message_expires_at = None;
            }
        }
    }

    pub(crate) fn apply_effect_result(&mut self, result: EffectResult) {
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
            }
        }

        let mut root = self.app.accounts_root_state();
        reducer::root::reduce(&mut root, &Action::EffectResult(result));
        root.set_accounts_quota_loading(!self.interactive_quota_refreshes.is_empty());
        self.app.apply_accounts_root_state(&root);
        self.accounts_status_message_expires_at = root
            .accounts()
            .status_message()
            .map(|_| Instant::now() + Duration::from_secs(5));

        if let Some((provider, profile_id, quota)) = refreshed_quota {
            self.persist_refreshed_quota_cache(provider, &profile_id, quota);
        }
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn apply_accounts_effect_result(&mut self, result: EffectResult) {
        self.apply_effect_result(result);
    }

    pub(crate) async fn run_accounts_reducer_action(&mut self, action: AccountsAction) {
        let mut root = self.app.accounts_root_state();
        let effects = reducer::root::reduce(&mut root, &Action::Accounts(action));
        self.app.apply_accounts_root_state(&root);
        self.accounts_status_message_expires_at = root
            .accounts()
            .status_message()
            .map(|_| Instant::now() + Duration::from_secs(5));

        for effect in effects {
            let result = dispatch_effect(&self.config, &self.stores, effect).await;
            self.apply_effect_result(result);
        }
    }

    pub(crate) async fn run_accounts_effect(&mut self, effect: Effect) {
        let result = dispatch_effect(&self.config, &self.stores, effect).await;
        self.apply_effect_result(result);
    }

    pub(crate) async fn initialize_accounts(&mut self) {
        self.run_accounts_effect(Effect::LoadProfiles).await;
    }

    pub(crate) async fn dispatch_action(&mut self, action: Action) -> bool {
        match action {
            Action::QuitRequested => false,
            Action::Workspace(action) => {
                self.app.run_workspace_reducer_action(action);
                true
            }
            Action::Overview(action) => {
                self.app.run_overview_reducer_action(action);
                true
            }
            Action::Accounts(action) => {
                self.run_accounts_reducer_action(action).await;
                true
            }
            Action::System(action) => {
                self.app.run_system_reducer_action(action);
                true
            }
            Action::EffectResult(result) => {
                self.apply_effect_result(result);
                true
            }
        }
    }

    fn persist_refreshed_quota_cache(
        &self,
        provider: Provider,
        profile_id: &str,
        quota: QuotaSnapshot,
    ) {
        let (path, label) = match provider {
            Provider::Codex => (&self.config.codex_quota_cache_file, "codex"),
            Provider::Claude => (&self.config.claude_quota_cache_file, "claude"),
        };
        let cache = Some(quota_cache_for_provider(
            self.app.root().accounts().quota_by_key(),
            provider,
        ));

        if let Err(error) = quota_service::persist_quota_snapshot(cache, profile_id, quota, path) {
            tracing::warn!("failed to save {label} quota cache: {error}");
        }
    }
}

impl Deref for RuntimeSession {
    type Target = AppState;

    fn deref(&self) -> &Self::Target {
        &self.app
    }
}

impl DerefMut for RuntimeSession {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.app
    }
}

fn quota_cache_for_provider(
    quota_by_key: &std::collections::BTreeMap<AccountKey, QuotaSnapshot>,
    provider: Provider,
) -> QuotaCache {
    let mut cache = QuotaCache::default();
    for (key, quota) in quota_by_key {
        if key.provider == provider {
            cache
                .by_profile
                .insert(key.profile_id.to_string(), quota.clone());
        }
    }
    cache
}
