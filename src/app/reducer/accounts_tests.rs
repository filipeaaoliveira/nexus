use super::*;
use crate::app::effect::{AppError, RefreshOrigin, StorageError};
use crate::app::state::{AccountKey, AccountsMode, EditableProfile};
use crate::claude_profiles::profile::StoredClaudeProfile;
use crate::codex_profiles::profile::StoredCodexProfile;
use crate::domain::accounts::ProfileId;
use crate::domain::accounts::{Confidence, DomainProfile, LimitState, QuotaSnapshot};
use chrono::Utc;

// ── helpers ──────────────────────────────────────────────────────

fn codex_profile(id: &str) -> StoredCodexProfile {
    let now = Utc::now();
    StoredCodexProfile {
        id: id.to_string(),
        label: format!("Codex {id}"),
        email: Some(format!("{id}@example.com")),
        plan: Some("pro".to_string()),
        notes: Some("notes".to_string()),
        enabled: true,
        last_used: None,
        created_at: now,
        updated_at: now,
    }
}

fn claude_profile(id: &str) -> StoredClaudeProfile {
    let now = Utc::now();
    StoredClaudeProfile {
        id: id.to_string(),
        label: format!("Claude {id}"),
        email: Some(format!("{id}@example.com")),
        plan: Some("max".to_string()),
        notes: Some("notes".to_string()),
        enabled: true,
        last_used: None,
        created_at: now,
        updated_at: now,
    }
}

fn quota_snapshot() -> QuotaSnapshot {
    QuotaSnapshot::legacy_fallback(
        Utc::now(),
        Confidence::Exact,
        LimitState::Available,
        None,
        Some(42),
        LimitState::Available,
        None,
        Some(84),
        "test quota".to_string(),
    )
}

fn make_entries() -> Vec<AccountEntry> {
    vec![
        AccountEntry {
            provider: Provider::Codex,
            profile_id: "codex-01".into(),
        },
        AccountEntry {
            provider: Provider::Codex,
            profile_id: "codex-02".into(),
        },
        AccountEntry {
            provider: Provider::Claude,
            profile_id: "claude-01".into(),
        },
    ]
}

fn make_profiles() -> Vec<DomainProfile> {
    vec![
        DomainProfile::from(codex_profile("codex-01")),
        DomainProfile::from(codex_profile("codex-02")),
        DomainProfile::from(claude_profile("claude-01")),
    ]
}

/// Load profiles into state via the reducer, returning the state and workflow.
fn load_profiles(
    entries: Vec<AccountEntry>,
    profiles: Vec<DomainProfile>,
) -> (AccountsState, Workflow) {
    let mut state = AccountsState::default();
    let mut workflow = Workflow::LoadingProfiles;
    reduce_result(
        &mut state,
        &mut workflow,
        &EffectResult::ProfilesLoaded { profiles, entries },
    );
    (state, workflow)
}

// ── invariant assertion ─────────────────────────────────────────

/// Checks structural invariants that must hold after every reducer step.
fn assert_invariants(state: &AccountsState) {
    // selected_index must be None or within visible bounds
    let visible_len = state.visible_entry_indices().len();
    if let Some(idx) = state.selected_index {
        assert!(
            idx < visible_len,
            "selected_index ({idx}) out of bounds for {} visible entries",
            visible_len,
        );
    }

    // Every key in profiles_by_key must have a matching entry
    for key in state.profiles_by_key.keys() {
        assert!(
            state
                .entries
                .iter()
                .any(|e| e.provider == key.provider && key.profile_id == e.profile_id),
            "profiles_by_key contains {:?} with no matching entry",
            key,
        );
    }

    // Every key in quota_by_key must have a matching entry
    for key in state.quota_by_key.keys() {
        assert!(
            state
                .entries
                .iter()
                .any(|e| e.provider == key.provider && key.profile_id == e.profile_id),
            "quota_by_key contains {:?} with no matching entry",
            key,
        );
    }
}

// ── basic reducer tests with invariant checks ───────────────────

#[test]
fn select_next_wraps_around() {
    let (mut state, mut wf) = load_profiles(make_entries(), make_profiles());
    assert_invariants(&state);

    for _ in 0..4 {
        reduce(&mut state, &mut wf, &AccountsAction::SelectNext);
        assert_invariants(&state);
    }

    // After 4 SelectNext on 3 items: should wrap to index 1
    assert_eq!(state.selected_index, Some(1));
}

#[test]
fn select_previous_wraps_around() {
    let (mut state, mut wf) = load_profiles(make_entries(), make_profiles());
    assert_invariants(&state);

    reduce(&mut state, &mut wf, &AccountsAction::SelectPrevious);
    assert_invariants(&state);

    // From clamped 0, previous wraps to last
    assert_eq!(state.selected_index, Some(2));
}

#[test]
fn selection_on_empty_state() {
    let mut state = AccountsState::default();
    let mut wf = Workflow::Idle;

    reduce(&mut state, &mut wf, &AccountsAction::SelectNext);
    assert_invariants(&state);
    assert_eq!(state.selected_index, None);

    reduce(&mut state, &mut wf, &AccountsAction::SelectPrevious);
    assert_invariants(&state);
    assert_eq!(state.selected_index, None);
}

#[test]
fn set_provider_filter_clamps_selection() {
    let (mut state, mut wf) = load_profiles(make_entries(), make_profiles());

    // After load, clamp_selection sets selected_index to Some(0).
    // Two SelectNext moves to index 2 (the Claude profile).
    reduce(&mut state, &mut wf, &AccountsAction::SelectNext);
    reduce(&mut state, &mut wf, &AccountsAction::SelectNext);
    assert_eq!(state.selected_index, Some(2));

    // Filter to Codex only (2 items) -- selection should clamp
    reduce(
        &mut state,
        &mut wf,
        &AccountsAction::SetProviderFilter(ProviderFilter::Codex),
    );
    assert_invariants(&state);
}

#[test]
fn profiles_loaded_resets_workflow_to_idle() {
    let (state, wf) = load_profiles(make_entries(), make_profiles());
    assert_eq!(wf, Workflow::Idle);
    assert_eq!(state.entries.len(), 3);
    assert_invariants(&state);
}

#[test]
fn profile_deleted_removes_entry_and_maps() {
    let (mut state, mut wf) = load_profiles(make_entries(), make_profiles());
    // Add quota for codex-01
    state.quota_by_key.insert(
        AccountKey::new(Provider::Codex, ProfileId::from("codex-01")),
        quota_snapshot(),
    );
    assert_invariants(&state);

    reduce_result(
        &mut state,
        &mut wf,
        &EffectResult::ProfileDeleted {
            entry: AccountEntry {
                provider: Provider::Codex,
                profile_id: "codex-01".into(),
            },
        },
    );
    assert_invariants(&state);
    assert_eq!(state.entries.len(), 2);
    assert!(!state
        .profiles_by_key
        .contains_key(&AccountKey::new(Provider::Codex, "codex-01")));
    assert!(!state
        .quota_by_key
        .contains_key(&AccountKey::new(Provider::Codex, "codex-01")));
}

#[test]
fn profile_imported_adds_entry_and_profile() {
    let (mut state, mut wf) = load_profiles(make_entries(), make_profiles());
    assert_invariants(&state);

    reduce_result(
        &mut state,
        &mut wf,
        &EffectResult::ProfileImported {
            profile: DomainProfile::from(codex_profile("codex-03")),
        },
    );
    assert_invariants(&state);
    assert_eq!(state.entries.len(), 4);
}

#[test]
fn failed_effect_sets_status_message() {
    let (mut state, mut wf) = load_profiles(make_entries(), make_profiles());
    reduce_result(
        &mut state,
        &mut wf,
        &EffectResult::Failed {
            effect: Effect::LoadProfiles,
            error: AppError::Storage(StorageError::Io {
                detail: "disk full".into(),
            }),
        },
    );
    assert_invariants(&state);
    assert_eq!(
        state.status_message.as_deref(),
        Some("I/O error: disk full")
    );
}

// ── multi-step sequence tests ───────────────────────────────────

#[test]
fn sequence_load_delete_first_select_next() {
    let (mut state, mut wf) = load_profiles(make_entries(), make_profiles());
    assert_invariants(&state);

    // Delete the first entry (codex-01)
    reduce_result(
        &mut state,
        &mut wf,
        &EffectResult::ProfileDeleted {
            entry: AccountEntry {
                provider: Provider::Codex,
                profile_id: "codex-01".into(),
            },
        },
    );
    assert_invariants(&state);

    // Navigate
    reduce(&mut state, &mut wf, &AccountsAction::SelectNext);
    assert_invariants(&state);
    reduce(&mut state, &mut wf, &AccountsAction::SelectNext);
    assert_invariants(&state);
}

#[test]
fn sequence_filter_to_codex_delete_last_codex_profile() {
    let (mut state, mut wf) = load_profiles(make_entries(), make_profiles());

    // Filter to Codex
    reduce(
        &mut state,
        &mut wf,
        &AccountsAction::SetProviderFilter(ProviderFilter::Codex),
    );
    assert_invariants(&state);
    assert_eq!(state.visible_entry_indices().len(), 2);

    // Select last Codex profile
    reduce(&mut state, &mut wf, &AccountsAction::SelectNext);
    assert_invariants(&state);

    // Delete codex-02
    reduce_result(
        &mut state,
        &mut wf,
        &EffectResult::ProfileDeleted {
            entry: AccountEntry {
                provider: Provider::Codex,
                profile_id: "codex-02".into(),
            },
        },
    );
    assert_invariants(&state);
    assert_eq!(state.visible_entry_indices().len(), 1);

    // Delete codex-01 (last remaining Codex)
    reduce_result(
        &mut state,
        &mut wf,
        &EffectResult::ProfileDeleted {
            entry: AccountEntry {
                provider: Provider::Codex,
                profile_id: "codex-01".into(),
            },
        },
    );
    assert_invariants(&state);
    assert_eq!(state.visible_entry_indices().len(), 0);
    assert_eq!(state.selected_index, None);
}

#[test]
fn sequence_load_start_editing_confirm_save() {
    let (mut state, mut wf) = load_profiles(make_entries(), make_profiles());
    assert_invariants(&state);

    // Advance selection (moves from index 0 to index 1)
    reduce(&mut state, &mut wf, &AccountsAction::SelectNext);
    assert_invariants(&state);

    // Start editing
    let draft = EditableProfile {
        label: "My Label".into(),
        notes: "My Notes".into(),
        enabled: true,
    };
    reduce(
        &mut state,
        &mut wf,
        &AccountsAction::StartEditing(draft.clone()),
    );
    assert_invariants(&state);
    assert!(matches!(state.mode, AccountsMode::Editing { .. }));

    // Confirm -> triggers save effect
    let effects = reduce(&mut state, &mut wf, &AccountsAction::Confirm);
    assert_invariants(&state);
    assert!(!effects.is_empty());
    assert!(matches!(wf, Workflow::SavingProfile { .. }));

    // Simulate save result
    reduce_result(
        &mut state,
        &mut wf,
        &EffectResult::ProfileSaved {
            profile: DomainProfile::from(codex_profile("codex-01")),
        },
    );
    assert_invariants(&state);
    assert_eq!(wf, Workflow::Idle);
    assert_eq!(state.mode, AccountsMode::Normal);
}

#[test]
fn sequence_import_then_immediately_delete() {
    let (mut state, mut wf) = load_profiles(make_entries(), make_profiles());
    let initial_count = state.entries.len();
    assert_invariants(&state);

    // Import a new profile
    reduce_result(
        &mut state,
        &mut wf,
        &EffectResult::ProfileImported {
            profile: DomainProfile::from(claude_profile("claude-02")),
        },
    );
    assert_invariants(&state);
    assert_eq!(state.entries.len(), initial_count + 1);

    // Immediately delete it
    reduce_result(
        &mut state,
        &mut wf,
        &EffectResult::ProfileDeleted {
            entry: AccountEntry {
                provider: Provider::Claude,
                profile_id: "claude-02".into(),
            },
        },
    );
    assert_invariants(&state);
    assert_eq!(state.entries.len(), initial_count);
}

#[test]
fn sequence_delete_all_profiles_one_by_one() {
    let (mut state, mut wf) = load_profiles(make_entries(), make_profiles());
    assert_invariants(&state);

    for entry in make_entries() {
        reduce_result(&mut state, &mut wf, &EffectResult::ProfileDeleted { entry });
        assert_invariants(&state);
    }

    assert!(state.entries.is_empty());
    assert!(state.profiles_by_key.is_empty());
    assert_eq!(state.selected_index, None);
}

#[test]
fn sequence_quota_refresh_then_delete() {
    let (mut state, mut wf) = load_profiles(make_entries(), make_profiles());

    // Refresh quota for codex-01
    reduce_result(
        &mut state,
        &mut wf,
        &EffectResult::CodexQuotaRefreshed {
            profile_id: "codex-01".into(),
            quota: quota_snapshot(),
            origin: RefreshOrigin::Manual,
        },
    );
    assert_invariants(&state);

    // Delete codex-01 -- quota should be cleaned up
    reduce_result(
        &mut state,
        &mut wf,
        &EffectResult::ProfileDeleted {
            entry: AccountEntry {
                provider: Provider::Codex,
                profile_id: "codex-01".into(),
            },
        },
    );
    assert_invariants(&state);
}

// ── workflow transition test ─────────────────────────────────────

#[test]
fn workflow_idle_to_saving_to_idle() {
    let (mut state, mut wf) = load_profiles(make_entries(), make_profiles());
    assert_eq!(wf, Workflow::Idle);

    // Select and start editing
    reduce(&mut state, &mut wf, &AccountsAction::SelectNext);
    reduce(
        &mut state,
        &mut wf,
        &AccountsAction::StartEditing(EditableProfile::default()),
    );

    // Confirm triggers SavingProfile workflow
    let _effects = reduce(&mut state, &mut wf, &AccountsAction::Confirm);
    assert!(matches!(wf, Workflow::SavingProfile { .. }));

    // Effect result returns workflow to Idle
    reduce_result(
        &mut state,
        &mut wf,
        &EffectResult::ProfileSaved {
            profile: DomainProfile::from(codex_profile("codex-01")),
        },
    );
    assert_eq!(wf, Workflow::Idle);
    assert_invariants(&state);
}

#[test]
fn workflow_idle_to_deleting_to_idle() {
    let (mut state, mut wf) = load_profiles(make_entries(), make_profiles());
    assert_eq!(wf, Workflow::Idle);

    // Select first and request delete confirmation
    reduce(&mut state, &mut wf, &AccountsAction::SelectNext);
    reduce(
        &mut state,
        &mut wf,
        &AccountsAction::RequestConfirmation(
            crate::app::state::AccountsConfirmAction::DeleteProfile,
        ),
    );
    assert!(matches!(state.mode, AccountsMode::Confirming { .. }));

    // Confirm triggers DeletingProfile workflow
    let _effects = reduce(&mut state, &mut wf, &AccountsAction::Confirm);
    assert!(matches!(wf, Workflow::DeletingProfile { .. }));

    // Effect result returns workflow to Idle
    reduce_result(
        &mut state,
        &mut wf,
        &EffectResult::ProfileDeleted {
            entry: AccountEntry {
                provider: Provider::Codex,
                profile_id: "codex-01".into(),
            },
        },
    );
    assert_eq!(wf, Workflow::Idle);
    assert_invariants(&state);
}

#[test]
fn workflow_idle_to_writing_home_to_idle() {
    let (mut state, mut wf) = load_profiles(make_entries(), make_profiles());
    assert_eq!(wf, Workflow::Idle);

    // Select first and request write-to-home confirmation
    reduce(&mut state, &mut wf, &AccountsAction::SelectNext);
    reduce(
        &mut state,
        &mut wf,
        &AccountsAction::RequestConfirmation(
            crate::app::state::AccountsConfirmAction::WriteProfileToHome,
        ),
    );

    // Confirm triggers WritingProfileToHome workflow
    let _effects = reduce(&mut state, &mut wf, &AccountsAction::Confirm);
    assert!(matches!(wf, Workflow::WritingProfileToHome { .. }));

    // Effect result returns workflow to Idle
    reduce_result(
        &mut state,
        &mut wf,
        &EffectResult::ProfileWrittenToHome {
            entry: AccountEntry {
                provider: Provider::Codex,
                profile_id: "codex-01".into(),
            },
        },
    );
    assert_eq!(wf, Workflow::Idle);
    assert_invariants(&state);
}

#[test]
fn workflow_failure_returns_to_idle() {
    let (mut state, mut wf) = load_profiles(make_entries(), make_profiles());

    // Select and start editing, then confirm
    reduce(&mut state, &mut wf, &AccountsAction::SelectNext);
    reduce(
        &mut state,
        &mut wf,
        &AccountsAction::StartEditing(EditableProfile::default()),
    );
    let effects = reduce(&mut state, &mut wf, &AccountsAction::Confirm);
    assert!(matches!(wf, Workflow::SavingProfile { .. }));

    // Simulate failure
    reduce_result(
        &mut state,
        &mut wf,
        &EffectResult::Failed {
            effect: effects.into_iter().next().unwrap(),
            error: AppError::Storage(StorageError::Io {
                detail: "write failed".into(),
            }),
        },
    );
    assert_eq!(wf, Workflow::Idle);
    assert_invariants(&state);
}
