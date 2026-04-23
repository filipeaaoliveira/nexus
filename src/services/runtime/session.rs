use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::app::dispatch::effects::dispatch_effect;
use crate::app::state::AccountKey;
use crate::app::{
    reducer, AccountsAction, Action, AppState, Effect, EffectResult, Provider, Workspace,
};
use crate::config::Config;
use crate::services::accounts::provider::ConfigStoreProvider;
use crate::services::accounts::quota as quota_service;
use nexus_domain::accounts::{ProfileId, QuotaCache, QuotaSnapshot};

pub(crate) struct RuntimeSession {
    app: AppState,
    config: Arc<Config>,
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
            config: Arc::new(config),
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

    pub(crate) fn config_arc(&self) -> Arc<Config> {
        Arc::clone(&self.config)
    }

    pub(crate) fn started_at(&self) -> Instant {
        self.started_at
    }

    pub(crate) fn begin_manual_quota_refresh(&mut self, provider: Provider, profile_id: &str) {
        self.app.set_accounts_quota_loading(true);
        self.interactive_quota_refreshes.insert(AccountKey::new(
            provider,
            ProfileId::new_unchecked(profile_id),
        ));
    }

    pub(crate) fn clear_expired_accounts_status(&mut self) {
        if let Some(expires_at) = self.accounts_status_message_expires_at {
            if Instant::now() >= expires_at {
                self.app.clear_accounts_status_message();
                self.accounts_status_message_expires_at = None;
            }
        }
    }

    pub(crate) fn apply_effect_result(&mut self, result: EffectResult) {
        let refreshed_quota = match &result {
            EffectResult::QuotaRefreshed {
                provider,
                profile_id,
                quota,
                ..
            } => Some((*provider, profile_id.clone(), quota.clone())),
            _ => None,
        };
        let refresh_state = match &result {
            EffectResult::QuotaRefreshed {
                provider,
                profile_id,
                origin,
                ..
            } => Some((AccountKey::new(*provider, profile_id.clone()), *origin)),
            EffectResult::Failed {
                effect:
                    Effect::RefreshQuota {
                        provider,
                        profile_id,
                        origin,
                    },
                ..
            } => Some((AccountKey::new(*provider, profile_id.clone()), *origin)),
            _ => None,
        };
        let background_refresh_failure = matches!(
            &result,
            EffectResult::Failed {
                effect: Effect::RefreshQuota { .. },
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

        reducer::root::reduce(&mut self.app, &Action::EffectResult(result));
        self.app
            .set_accounts_quota_loading(!self.interactive_quota_refreshes.is_empty());
        self.accounts_status_message_expires_at = self
            .app
            .accounts_status_message()
            .map(|_| Instant::now() + Duration::from_secs(5));

        if let Some((provider, profile_id, quota)) = refreshed_quota {
            self.persist_refreshed_quota_cache(provider, &profile_id, quota);
        }
    }

    pub(crate) async fn run_accounts_reducer_action(&mut self, action: AccountsAction) {
        let effects = reducer::root::reduce(&mut self.app, &Action::Accounts(action));
        self.accounts_status_message_expires_at = self
            .app
            .accounts_status_message()
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
            Action::Workspace(_) | Action::Overview(_) | Action::System(_) => {
                reducer::root::reduce(&mut self.app, &action);
                if matches!(action, Action::System(_)) {
                    self.app.clamp_help_scroll();
                }
                true
            }
            Action::Accounts(action) => {
                self.run_accounts_reducer_action(action).await;
                true
            }
            Action::EffectResult(result) => {
                self.apply_effect_result(result);
                true
            }
            Action::AgentEvent { .. } => {
                reducer::root::reduce(&mut self.app, &action);
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
            _ => unreachable!(),
        };
        let cache = Some(quota_cache_for_provider(
            self.app.root().accounts().quota_by_key(),
            provider,
        ));

        if let Err(error) = quota_service::persist_quota_snapshot(cache, profile_id, quota, path) {
            tracing::warn!("failed to save {label} quota cache: {error}");
        }
    }

    // Delegation methods for commonly used AppState methods
    pub(crate) fn active_tab(&self) -> Workspace {
        self.app.active_tab()
    }

    pub(crate) fn show_help(&self) -> bool {
        self.app.show_help()
    }

    pub(crate) fn help_scroll(&self) -> u16 {
        self.app.help_scroll()
    }

    pub(crate) fn root(&self) -> &crate::app::RootState {
        self.app.root()
    }

    pub(crate) fn toggle_help(&mut self) {
        self.app.toggle_help();
    }

    pub(crate) fn dismiss_overlay(&mut self) {
        self.app.dismiss_overlay();
    }

    pub(crate) fn dismiss_accounts_overlay(&mut self) {
        self.app.dismiss_accounts_overlay();
    }

    pub(crate) fn start_rename(&mut self) {
        self.app.start_rename();
    }

    pub(crate) fn remove_selected(&mut self) {
        self.app.remove_selected();
    }

    pub(crate) fn remove_exited(&mut self) {
        self.app.remove_exited();
    }

    pub(crate) fn selected_account(&self) -> Option<(Provider, &str)> {
        self.app.selected_account()
    }

    pub(crate) fn primary_detector_issue(&self) -> Option<&str> {
        self.app.primary_detector_issue()
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
