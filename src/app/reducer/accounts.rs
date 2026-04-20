use crate::app::action::AccountsAction;
use crate::app::effect::{Effect, EffectResult};
use crate::app::state::{AccountKey, AccountsMode, AccountsState, Workflow};
use nexus_domain::accounts::{AccountEntry, DomainProfile, Provider, ProviderFilter};

pub fn reduce(
    state: &mut AccountsState,
    workflow: &mut Workflow,
    action: &AccountsAction,
) -> Vec<Effect> {
    match action {
        AccountsAction::SelectNext => select_next(state),
        AccountsAction::SelectPrevious => select_previous(state),
        AccountsAction::SetProviderFilter(filter) => {
            state.provider_filter = *filter;
            state.clamp_selection();
        }
        AccountsAction::StartFiltering => {
            state.mode = AccountsMode::Filtering {
                input: String::new(),
            };
        }
        AccountsAction::UpdateFilter(input) => {
            if matches!(state.mode, AccountsMode::Filtering { .. }) {
                state.mode = AccountsMode::Filtering {
                    input: input.clone(),
                };
            }
        }
        AccountsAction::StartEditing(draft) => {
            state.mode = AccountsMode::Editing {
                draft: draft.clone(),
                field: crate::app::state::AccountsEditField::Label,
            };
        }
        AccountsAction::UpdateEditBuffer(input) => update_edit_buffer(state, input),
        AccountsAction::ToggleEnabled => toggle_enabled(state),
        AccountsAction::RequestConfirmation(action) => {
            state.mode = AccountsMode::Confirming { action: *action };
        }
        AccountsAction::SetMode(mode) => state.mode = mode.clone(),
        AccountsAction::Confirm => return confirm(state, workflow),
        AccountsAction::CancelMode => state.mode = AccountsMode::Normal,
    }

    Vec::new()
}

pub fn reduce_result(
    state: &mut AccountsState,
    workflow: &mut Workflow,
    result: &EffectResult,
) -> bool {
    match result {
        EffectResult::ProfilesLoaded { profiles, entries } => {
            state.entries = entries.clone();
            state.profiles_by_key = profiles
                .iter()
                .cloned()
                .map(|profile| (AccountKey::for_profile(&profile), profile))
                .collect();
            state.clamp_selection();
            *workflow = Workflow::Idle;
            true
        }
        EffectResult::EditableProfileLoaded {
            provider,
            profile_id,
            draft,
        } => {
            focus_profile(state, &AccountKey::new(*provider, profile_id.clone()));
            state.mode = AccountsMode::Editing {
                draft: draft.clone(),
                field: crate::app::state::AccountsEditField::Label,
            };
            *workflow = Workflow::Idle;
            true
        }
        EffectResult::CodexQuotaRefreshed {
            profile_id, quota, ..
        } => {
            state.quota_by_key.insert(
                AccountKey::new(Provider::Codex, profile_id.clone()),
                quota.clone(),
            );
            *workflow = Workflow::Idle;
            true
        }
        EffectResult::ClaudeQuotaRefreshed {
            profile_id, quota, ..
        } => {
            state.quota_by_key.insert(
                AccountKey::new(Provider::Claude, profile_id.clone()),
                quota.clone(),
            );
            *workflow = Workflow::Idle;
            true
        }
        EffectResult::ProfileImported { profile } | EffectResult::ProfileSaved { profile } => {
            let imported_key = AccountKey::for_profile(profile);
            upsert_profile(state, profile.clone());
            state.status_message = Some(match result {
                EffectResult::ProfileImported { profile } => {
                    state.mode = AccountsMode::Normal;
                    focus_profile(state, &imported_key);
                    format!("Imported {}", profile.profile_id())
                }
                EffectResult::ProfileSaved { profile } => {
                    state.mode = AccountsMode::Normal;
                    format!("Saved profile {}", profile.profile_id())
                }
                _ => unreachable!(),
            });
            *workflow = Workflow::Idle;
            true
        }
        EffectResult::ProfileWrittenToHome { entry } => {
            state.status_message = Some(match entry.provider {
                Provider::Codex => format!("Wrote {} to ~/.codex/auth.json", entry.profile_id),
                Provider::Claude => {
                    format!("Wrote {} to ~/.claude/.credentials.json", entry.profile_id)
                }
            });
            *workflow = Workflow::Idle;
            true
        }
        EffectResult::ProfileDeleted { entry } => {
            remove_entry(state, entry);
            match entry.provider {
                Provider::Codex
                    if state.active_codex_profile_id.as_ref() == Some(&entry.profile_id) =>
                {
                    state.active_codex_profile_id = None;
                }
                Provider::Claude
                    if state.active_claude_profile_id.as_ref() == Some(&entry.profile_id) =>
                {
                    state.active_claude_profile_id = None;
                }
                _ => {}
            }
            state.status_message = Some(match entry.provider {
                Provider::Codex => format!("Deleted profile {}", entry.profile_id),
                Provider::Claude => format!("Deleted Claude {}", entry.profile_id),
            });
            state.mode = AccountsMode::Normal;
            *workflow = Workflow::Idle;
            true
        }
        EffectResult::Failed { effect, error } => {
            if is_accounts_effect(effect) {
                state.status_message = Some(error.to_string());
                if !matches!(effect, Effect::SaveProfileEdits { .. }) {
                    state.mode = AccountsMode::Normal;
                }
                *workflow = Workflow::Idle;
                true
            } else {
                false
            }
        }
    }
}

fn select_next(state: &mut AccountsState) {
    let len = state.visible_entry_indices().len();
    if len == 0 {
        state.selected_index = None;
        return;
    }
    state.selected_index = Some(match state.selected_index {
        Some(idx) => (idx + 1) % len,
        None => 0,
    });
}

fn select_previous(state: &mut AccountsState) {
    let len = state.visible_entry_indices().len();
    if len == 0 {
        state.selected_index = None;
        return;
    }
    state.selected_index = Some(match state.selected_index {
        Some(idx) => (idx + len - 1) % len,
        None => len - 1,
    });
}

fn update_edit_buffer(state: &mut AccountsState, input: &str) {
    if let AccountsMode::Editing { draft, field } = &mut state.mode {
        match field {
            crate::app::state::AccountsEditField::Label => draft.label = input.to_string(),
            crate::app::state::AccountsEditField::Notes => draft.notes = input.to_string(),
            crate::app::state::AccountsEditField::Enabled => {}
        }
    }
}

fn toggle_enabled(state: &mut AccountsState) {
    if let AccountsMode::Editing { draft, .. } = &mut state.mode {
        draft.enabled = !draft.enabled;
    }
}

fn confirm(state: &mut AccountsState, workflow: &mut Workflow) -> Vec<Effect> {
    match state.mode.clone() {
        AccountsMode::Filtering { .. } => {
            state.mode = AccountsMode::Normal;
            Vec::new()
        }
        AccountsMode::Editing { draft, .. } => {
            let Some(entry) = state.selected_entry().cloned() else {
                return Vec::new();
            };
            *workflow = Workflow::SavingProfile {
                provider: entry.provider,
                profile_id: entry.profile_id.clone(),
            };
            vec![Effect::SaveProfileEdits {
                provider: entry.provider,
                profile_id: entry.profile_id,
                draft,
            }]
        }
        AccountsMode::Confirming { action } => {
            let Some(entry) = state.selected_entry().cloned() else {
                return Vec::new();
            };
            state.mode = AccountsMode::Normal;
            match action {
                crate::app::state::AccountsConfirmAction::DeleteProfile => {
                    *workflow = Workflow::DeletingProfile {
                        provider: entry.provider,
                        profile_id: entry.profile_id.clone(),
                    };
                    vec![Effect::DeleteProfile {
                        provider: entry.provider,
                        profile_id: entry.profile_id,
                    }]
                }
                crate::app::state::AccountsConfirmAction::WriteProfileToHome => {
                    *workflow = Workflow::WritingProfileToHome {
                        provider: entry.provider,
                        profile_id: entry.profile_id.clone(),
                    };
                    vec![Effect::WriteProfileToHome {
                        provider: entry.provider,
                        profile_id: entry.profile_id,
                    }]
                }
            }
        }
        AccountsMode::Normal => Vec::new(),
    }
}

fn upsert_profile(state: &mut AccountsState, profile: DomainProfile) {
    let key = AccountKey::for_profile(&profile);
    if !state
        .entries
        .iter()
        .any(|entry| entry.provider == key.provider && key.profile_id == entry.profile_id)
    {
        state.entries.push(AccountEntry {
            provider: key.provider,
            profile_id: key.profile_id.clone(),
        });
    }
    state.profiles_by_key.insert(key, profile);
    state.clamp_selection();
}

fn remove_entry(state: &mut AccountsState, entry: &AccountEntry) {
    state.entries.retain(|candidate| {
        !(candidate.provider == entry.provider && candidate.profile_id == entry.profile_id)
    });
    let key = AccountKey::for_entry(entry);
    state.profiles_by_key.remove(&key);
    state.quota_by_key.remove(&key);
    state.clamp_selection();
}

fn focus_profile(state: &mut AccountsState, key: &AccountKey) {
    state.provider_filter = match key.provider {
        Provider::Codex => ProviderFilter::Codex,
        Provider::Claude => ProviderFilter::Claude,
    };
    let visible_indices = state.visible_entry_indices();
    state.selected_index = visible_indices.iter().position(|real_idx| {
        state.entries[*real_idx].provider == key.provider
            && key.profile_id == state.entries[*real_idx].profile_id
    });
    state.clamp_selection();
}

fn is_accounts_effect(effect: &Effect) -> bool {
    matches!(
        effect,
        Effect::LoadProfiles
            | Effect::LoadEditableProfile { .. }
            | Effect::RefreshCodexQuota { .. }
            | Effect::RefreshClaudeQuota { .. }
            | Effect::ImportCodexProfile
            | Effect::ImportClaudeProfile
            | Effect::SaveProfileEdits { .. }
            | Effect::WriteProfileToHome { .. }
            | Effect::DeleteProfile { .. }
    )
}

#[cfg(test)]
#[path = "accounts_tests.rs"]
mod tests;
