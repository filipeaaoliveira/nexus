use super::*;
use crate::services::accounts::{
    claude as claude_service, codex as codex_service, quota as quota_service,
    storage as storage_service,
};

impl AppState {
    pub fn handle_accounts_move_down(&mut self) {
        self.accounts_tab.next_profile();
    }

    pub fn handle_accounts_move_up(&mut self) {
        self.accounts_tab.previous_profile();
    }

    pub fn dismiss_accounts_overlay(&mut self) {
        self.show_help = false;
        self.help_scroll = 0;
        self.accounts_tab.filter_mode = false;
        self.accounts_tab.filter_input.clear();
        self.accounts_tab.pending_confirm = None;
        self.accounts_tab.cancel_edit();
    }

    pub fn start_accounts_filter(&mut self) {
        self.accounts_tab.filter_mode = true;
        self.accounts_tab.filter_input.clear();
    }

    pub fn accounts_filter_push_char(&mut self, c: char) {
        self.accounts_tab.filter_input.push(c);
    }

    pub fn accounts_filter_pop_char(&mut self) {
        self.accounts_tab.filter_input.pop();
    }

    pub fn confirm_accounts_filter(&mut self) {
        self.accounts_tab.filter_mode = false;
    }

    /// Execute whatever destructive action is waiting for confirmation.
    pub fn execute_pending_confirm(&mut self) -> Result<()> {
        let confirm = match self.accounts_tab.pending_confirm.take() {
            Some(c) => c,
            None => return Ok(()),
        };
        match confirm {
            ConfirmKind::DeleteProfile => match self.selected_account() {
                Some((Provider::Claude, _)) => self.delete_claude_profile()?,
                Some((Provider::Codex, _)) | None => self.delete_codex_profile()?,
            },
            ConfirmKind::WriteToHome => match self.selected_account() {
                Some((Provider::Claude, _)) => self.write_selected_claude_profile_to_home()?,
                Some((Provider::Codex, _)) | None => self.write_selected_profile_to_codex_home()?,
            },
        }
        Ok(())
    }

    pub fn start_accounts_edit(&mut self) {
        let (provider, profile_id) = match self.selected_account() {
            Some((p, id)) => (p, id.to_string()),
            None => return,
        };

        let mut edit_email = None;
        let mut edit_plan = None;
        let mut label = profile_id.clone();
        let mut notes = String::new();
        let mut enabled = true;

        match provider {
            Provider::Codex => {
                let profile_dir = self.config.codex_profiles_dir.join(&profile_id);
                if let Ok(profile) = storage_service::load_codex_profile(&profile_dir) {
                    label = profile.label;
                    edit_email = profile.email;
                    edit_plan = profile.plan;
                    notes = profile.notes.unwrap_or_default();
                    enabled = profile.enabled;
                }
            }
            Provider::Claude => {
                let profile_dir = self.config.claude_profiles_dir.join(&profile_id);
                if let Ok(profile) = storage_service::load_claude_profile(&profile_dir) {
                    label = profile.label;
                    edit_email = profile.email;
                    edit_plan = profile.plan;
                    notes = profile.notes.unwrap_or_default();
                    enabled = profile.enabled;
                }
            }
        }

        self.accounts_tab.start_edit(
            &label,
            edit_email.as_deref(),
            edit_plan.as_deref(),
            &notes,
            enabled,
        );
    }

    pub fn confirm_accounts_edit(&mut self) -> Result<()> {
        if !self.accounts_tab.edit_mode {
            return Ok(());
        }

        let (provider, profile_id) = match self.selected_account() {
            Some((p, id)) => (p, id.to_string()),
            None => return Err(anyhow!("no profile selected")),
        };

        match provider {
            Provider::Codex => {
                let profile_dir = self.config.codex_profiles_dir.join(&profile_id);
                storage_service::save_codex_profile_metadata(
                    &profile_dir,
                    &profile_id,
                    &self.accounts_tab.edit_label,
                    if self.accounts_tab.edit_notes.is_empty() {
                        None
                    } else {
                        Some(self.accounts_tab.edit_notes.clone())
                    },
                    self.accounts_tab.edit_enabled,
                )?;
            }
            Provider::Claude => {
                let profile_dir = self.config.claude_profiles_dir.join(&profile_id);
                storage_service::save_claude_profile_edits(
                    &profile_dir,
                    &profile_id,
                    &self.accounts_tab.edit_label,
                    if self.accounts_tab.edit_notes.is_empty() {
                        None
                    } else {
                        Some(self.accounts_tab.edit_notes.clone())
                    },
                    self.accounts_tab.edit_enabled,
                )?;
            }
        }

        self.accounts_tab.edit_mode = false;
        self.accounts_tab
            .set_status(format!("Saved profile {}", profile_id));
        Ok(())
    }

    pub fn delete_codex_profile(&mut self) -> Result<()> {
        let profile_id = match self.selected_codex_profile_id() {
            Some(id) => id.to_string(),
            None => return Err(anyhow!("no profile selected")),
        };

        let profile_dir = self.config.codex_profiles_dir.join(&profile_id);
        storage_service::delete_profile_dir(&profile_dir)?;

        if let Some(active) = &self.accounts_tab.codex_active_profile_id {
            if active == &profile_id {
                self.accounts_tab.codex_active_profile_id = None;
            }
        }

        self.accounts_tab
            .codex_profile_ids
            .retain(|id| id != &profile_id);
        self.clamp_accounts_selection();
        self.accounts_tab
            .set_status(format!("Deleted profile {}", profile_id));
        Ok(())
    }

    pub fn delete_claude_profile(&mut self) -> anyhow::Result<()> {
        let profile_id = self
            .selected_claude_profile_id()
            .map(|s| s.to_string())
            .ok_or_else(|| anyhow::anyhow!("no Claude profile selected"))?;

        let profile_dir = self.config.claude_profiles_dir.join(&profile_id);
        storage_service::delete_profile_dir(&profile_dir)?;

        self.accounts_tab
            .claude_profile_ids
            .retain(|id| id != &profile_id);
        self.clamp_accounts_selection();
        self.accounts_tab
            .set_status(format!("Deleted Claude {}", profile_id));
        Ok(())
    }

    pub fn cancel_accounts_edit(&mut self) {
        self.accounts_tab.cancel_edit();
    }

    pub fn dismiss_accounts_edit(&mut self) {
        self.cancel_accounts_edit();
    }

    pub fn accounts_edit_push_char(&mut self, c: char) {
        match self.accounts_tab.edit_field {
            EditField::Label => self.accounts_tab.edit_label.push(c),
            EditField::Notes => self.accounts_tab.edit_notes.push(c),
            EditField::Enabled => {}
        }
    }

    pub fn accounts_edit_pop_char(&mut self) {
        match self.accounts_tab.edit_field {
            EditField::Label => {
                self.accounts_tab.edit_label.pop();
            }
            EditField::Notes => {
                self.accounts_tab.edit_notes.pop();
            }
            EditField::Enabled => {}
        }
    }

    pub fn accounts_edit_toggle_enabled(&mut self) {
        self.accounts_tab.edit_enabled = !self.accounts_tab.edit_enabled;
    }

    pub fn accounts_edit_next_field(&mut self) {
        self.accounts_tab.edit_field = match self.accounts_tab.edit_field {
            EditField::Label => EditField::Notes,
            EditField::Notes => EditField::Enabled,
            EditField::Enabled => EditField::Label,
        };
    }

    pub fn accounts_edit_prev_field(&mut self) {
        self.accounts_tab.edit_field = match self.accounts_tab.edit_field {
            EditField::Label => EditField::Enabled,
            EditField::Notes => EditField::Label,
            EditField::Enabled => EditField::Notes,
        };
    }

    pub fn import_codex_auth(&mut self) -> Result<()> {
        let home = dirs::home_dir().ok_or_else(|| anyhow!("home directory not found"))?;
        let profile =
            codex_service::import_profile_from_home(&self.config.codex_profiles_dir, &home)?;

        self.reload_codex_profiles();
        self.select_codex_profile_id(&profile.id);
        self.accounts_tab
            .set_status(format!("Imported {}", profile.id));
        Ok(())
    }

    pub fn import_claude_auth(&mut self) -> anyhow::Result<()> {
        let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("home directory not found"))?;
        let profile =
            claude_service::import_profile_from_home(&self.config.claude_profiles_dir, &home)?;

        self.reload_claude_profiles();
        self.accounts_tab
            .set_status(format!("Imported Claude {}", profile.id));
        Ok(())
    }

    pub fn write_selected_claude_profile_to_home(&mut self) -> anyhow::Result<()> {
        let profile_id = self
            .selected_claude_profile_id()
            .map(|s| s.to_string())
            .ok_or_else(|| anyhow::anyhow!("no Claude profile selected"))?;
        let source = self.config.claude_profiles_dir.join(&profile_id);
        let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("home directory not found"))?;
        claude_service::write_profile_to_home(&source, &home)?;
        self.accounts_tab.set_status(format!(
            "Wrote {} to ~/.claude/.credentials.json",
            profile_id
        ));
        Ok(())
    }

    pub fn write_selected_profile_to_codex_home(&mut self) -> Result<()> {
        let active_profile_id = self
            .selected_account()
            .filter(|(p, _)| *p == Provider::Codex)
            .map(|(_, id)| id.to_string())
            .ok_or_else(|| anyhow!("no codex profile selected"))?;

        let profile_dir = self.config.codex_profiles_dir.join(&active_profile_id);
        let home = dirs::home_dir().ok_or_else(|| anyhow!("home directory not found"))?;
        codex_service::write_profile_to_home(&profile_dir, &home)?;

        self.accounts_tab
            .set_status(format!("Wrote {} to ~/.codex/auth.json", active_profile_id));
        Ok(())
    }

    pub fn cycle_accounts_filter(&mut self) {
        self.accounts_tab.active_filter = self.accounts_tab.active_filter.next();
        self.clamp_accounts_selection();
        self.accounts_tab.load_quota_from_cache();
    }

    pub fn reload_codex_profiles(&mut self) {
        // Skip if called within last 5 seconds to avoid repeated scans
        if self.codex_profiles_last_reload.get().elapsed() < std::time::Duration::from_secs(5) {
            return;
        }
        self.codex_profiles_last_reload
            .set(std::time::Instant::now());

        let selected_profile_id = self.selected_codex_profile_id().map(ToOwned::to_owned);
        let profiles_dir = self.config.codex_profiles_dir.clone();
        let scan = || storage_service::scan_codex_profile_ids(&profiles_dir);
        // block_in_place avoids starving the tokio scheduler on a multi-thread
        // runtime; fall back to a direct call in single-threaded contexts (tests).
        let profile_ids = if tokio::runtime::Handle::try_current().is_ok() {
            tokio::task::block_in_place(scan)
        } else {
            scan()
        };

        self.accounts_tab.codex_profile_ids = profile_ids;

        // After setting profile IDs, select and load quota from cache
        if let Some(profile_id) = selected_profile_id.as_deref() {
            self.select_codex_profile_id(profile_id);
        } else {
            self.clamp_accounts_selection();
            self.accounts_tab.load_quota_from_cache();
        }
    }

    pub fn reload_claude_profiles(&mut self) {
        if self.claude_profiles_last_reload.get().elapsed() < std::time::Duration::from_secs(5) {
            return;
        }
        self.claude_profiles_last_reload
            .set(std::time::Instant::now());

        let previously_selected = self.selected_claude_profile_id().map(|s| s.to_string());

        let profiles_dir = self.config.claude_profiles_dir.clone();
        let scan = || storage_service::scan_claude_profile_ids(&profiles_dir);
        let ids = if tokio::runtime::Handle::try_current().is_ok() {
            tokio::task::block_in_place(scan)
        } else {
            scan()
        };

        self.accounts_tab.claude_profile_ids = ids;

        // Re-select the previously selected Claude profile if it still exists
        if let Some(ref prev_id) = previously_selected {
            let list = build_unified_list(
                &self.accounts_tab.codex_profile_ids,
                &self.accounts_tab.claude_profile_ids,
                self.accounts_tab.active_filter,
            );
            if let Some(idx) = list
                .iter()
                .position(|e| e.provider == Provider::Claude && &e.profile_id == prev_id)
            {
                self.accounts_tab.selected_index = Some(idx);
            } else {
                self.clamp_accounts_selection();
            }
        } else {
            self.clamp_accounts_selection();
        }

        self.accounts_tab.load_quota_from_cache();
    }

    /// Load quota from cache for currently selected profile.
    pub fn load_quota_from_cache(&mut self) {
        self.accounts_tab.load_quota_from_cache();
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
        let list = build_unified_list(
            &self.accounts_tab.codex_profile_ids,
            &self.accounts_tab.claude_profile_ids,
            self.accounts_tab.active_filter,
        );
        let idx = self.accounts_tab.selected_index?;
        let entry = list.get(idx)?;
        let provider = entry.provider;
        let profile_id_ref = match provider {
            Provider::Codex => {
                let pos = self
                    .accounts_tab
                    .codex_profile_ids
                    .iter()
                    .position(|s| s == &entry.profile_id)?;
                self.accounts_tab.codex_profile_ids[pos].as_str()
            }
            Provider::Claude => {
                let pos = self
                    .accounts_tab
                    .claude_profile_ids
                    .iter()
                    .position(|s| s == &entry.profile_id)?;
                self.accounts_tab.claude_profile_ids[pos].as_str()
            }
        };
        Some((provider, profile_id_ref))
    }

    pub fn update_codex_quota_for_profile(&mut self, profile_id: &str, quota: QuotaSnapshot) {
        self.accounts_tab.quota_loading = false;
        let mut cache = self
            .accounts_tab
            .codex_quota_cache
            .take()
            .unwrap_or_default();
        cache
            .by_profile
            .insert(profile_id.to_string(), quota.clone());
        if let Err(e) = quota_service::save_quota_cache(&cache, &self.config.codex_quota_cache_file)
        {
            tracing::warn!("failed to save codex quota cache: {}", e);
        }
        self.accounts_tab.codex_quota_cache = Some(cache);
        if self.selected_codex_profile_id() == Some(profile_id) {
            self.accounts_tab.quota = Some(quota);
        }
    }

    pub fn update_claude_quota_for_profile(&mut self, profile_id: &str, quota: QuotaSnapshot) {
        self.accounts_tab.quota_loading = false;
        let mut cache = self
            .accounts_tab
            .claude_quota_cache
            .take()
            .unwrap_or_default();
        cache
            .by_profile
            .insert(profile_id.to_string(), quota.clone());
        if let Err(e) =
            quota_service::save_quota_cache(&cache, &self.config.claude_quota_cache_file)
        {
            tracing::warn!("failed to save claude quota cache: {}", e);
        }
        self.accounts_tab.claude_quota_cache = Some(cache);
        if self.selected_claude_profile_id() == Some(profile_id) {
            self.accounts_tab.quota = Some(quota);
        }
    }

    pub fn apply_quota_refresh_result(
        &mut self,
        provider: Provider,
        profile_id: &str,
        quota: Option<QuotaSnapshot>,
    ) {
        match (provider, quota) {
            (Provider::Codex, Some(quota)) => {
                self.update_codex_quota_for_profile(profile_id, quota)
            }
            (Provider::Claude, Some(quota)) => {
                self.update_claude_quota_for_profile(profile_id, quota)
            }
            (_, None) => self.accounts_tab.quota_loading = false,
        }
    }

    pub fn set_active_codex_profile(&mut self, profile_id: Option<String>) {
        self.accounts_tab.set_active(profile_id);
    }

    pub fn active_codex_profile_id(&self) -> Option<&str> {
        self.accounts_tab.codex_active_profile_id.as_deref()
    }

    pub(super) fn select_codex_profile_id(&mut self, profile_id: &str) {
        let list = build_unified_list(
            &self.accounts_tab.codex_profile_ids,
            &self.accounts_tab.claude_profile_ids,
            self.accounts_tab.active_filter,
        );
        self.accounts_tab.selected_index = list
            .iter()
            .position(|e| e.provider == Provider::Codex && e.profile_id == profile_id);
        self.clamp_accounts_selection();
    }

    pub(super) fn clamp_accounts_selection(&mut self) {
        let list = build_unified_list(
            &self.accounts_tab.codex_profile_ids,
            &self.accounts_tab.claude_profile_ids,
            self.accounts_tab.active_filter,
        );
        let len = list.len();
        self.accounts_tab.selected_index = match (self.accounts_tab.selected_index, len) {
            (_, 0) => None,
            (Some(i), len) if i < len => Some(i),
            _ => Some(0),
        };
    }
}
