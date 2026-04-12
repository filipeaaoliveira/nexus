use chrono::Utc;
use nexus::app::{
    reducer, AccountProfile, AccountsAction, AccountsConfirmAction, AccountsEditField,
    AccountsMode, Action, ActiveTab, AppState, DetectorAction, Effect, EffectResult,
    OverviewAction, OverviewMode, ProfileDraft, Provider, RootState, Workflow,
};
use nexus::codex_profiles::quota::{Confidence, LimitState, QuotaSnapshot};
use nexus::config::Config;
use nexus::domain::accounts::{AccountEntry, ProviderFilter};
use nexus::domain::agents::{
    ActivityEventKind, AgentId, AgentIdentity, AgentTool, NormalizedReason, NormalizedState,
    NormalizedStatus, PrimaryStatus, SortMode, Subagent, TodoItem, TodoPriority, TodoStatus,
};
use nexus::event::AgentEvent;
use nexus::ui::{agent_list, footer, help};
use std::path::PathBuf;
use tempfile::tempdir;

fn make_state() -> (tempfile::TempDir, AppState) {
    let temp = tempdir().expect("tempdir");
    let root = temp.path().to_path_buf();

    let mut config = Config::default();
    config.codex_management_dir = root.join("codex-management");
    config.codex_profiles_dir = config.codex_management_dir.join("profiles");
    config.codex_repos_dir = config.codex_management_dir.join("repos");
    config.codex_runtime_dir = config.codex_management_dir.join("runtime");
    config.codex_quota_cache_file = config.codex_management_dir.join("quota-cache.json");
    config.codex_data_dir = root.join("codex-monitor");
    config.claude_data_dir = root.join("claude");
    config.opencode_data_dir = root.join("opencode");

    (temp, AppState::new(config))
}

fn quota_snapshot(summary: &str) -> QuotaSnapshot {
    QuotaSnapshot::legacy_fallback(
        Utc::now(),
        Confidence::Exact,
        LimitState::Available,
        None,
        Some(25),
        LimitState::Available,
        None,
        Some(40),
        summary.to_string(),
    )
}

fn discovered(
    tool: AgentTool,
    pid: u32,
    cwd: impl Into<PathBuf>,
    started_at: Option<chrono::DateTime<chrono::Local>>,
) -> AgentEvent {
    AgentEvent::Discovered {
        id: AgentId::new(tool, pid),
        pid: Some(pid),
        cwd: cwd.into(),
        started_at,
    }
}

fn session_info(
    tool: AgentTool,
    pid: u32,
    session_id: impl Into<String>,
    branch: Option<String>,
    is_worktree: bool,
    started_at: Option<chrono::DateTime<chrono::Local>>,
) -> AgentEvent {
    let session_id = session_id.into();
    let id = if tool == AgentTool::Codex && !session_id.is_empty() {
        AgentId::session(tool, session_id.clone())
    } else {
        AgentId::new(tool, pid)
    };
    AgentEvent::SessionInfo {
        id,
        pid: Some(pid),
        session_id,
        branch,
        is_worktree,
        started_at,
    }
}

fn status_changed(tool: AgentTool, pid: u32, old: PrimaryStatus, new: PrimaryStatus) -> AgentEvent {
    AgentEvent::StatusChanged {
        id: AgentId::new(tool, pid),
        pid: Some(pid),
        old,
        new,
    }
}

fn normalized_state_changed(tool: AgentTool, pid: u32, state: NormalizedState) -> AgentEvent {
    AgentEvent::NormalizedStateChanged {
        id: AgentId::new(tool, pid),
        pid: Some(pid),
        state,
    }
}

fn process_exited(tool: AgentTool, pid: u32) -> AgentEvent {
    AgentEvent::ProcessExited {
        id: AgentId::new(tool, pid),
        pid: Some(pid),
    }
}

fn subagent_spawned(tool: AgentTool, pid: u32, subagent: Subagent) -> AgentEvent {
    AgentEvent::SubagentSpawned {
        id: AgentId::new(tool, pid),
        pid: Some(pid),
        subagent,
    }
}

fn subagent_completed(tool: AgentTool, pid: u32, subagent_name: String) -> AgentEvent {
    AgentEvent::SubagentCompleted {
        id: AgentId::new(tool, pid),
        pid: Some(pid),
        subagent_name,
    }
}

fn todos_updated(tool: AgentTool, pid: u32, todos: Vec<TodoItem>) -> AgentEvent {
    AgentEvent::TodosUpdated {
        id: AgentId::new(tool, pid),
        pid: Some(pid),
        todos,
    }
}

fn overview_root_state() -> RootState {
    RootState::default()
}

fn overview_detector_discovered(
    tool: AgentTool,
    pid: u32,
    cwd: impl Into<PathBuf>,
    started_at: Option<chrono::DateTime<chrono::Local>>,
) -> Action {
    Action::Detector(DetectorAction::Discovered {
        id: AgentId::new(tool, pid),
        pid: Some(pid),
        cwd: cwd.into(),
        started_at,
    })
}

fn overview_detector_session_info(
    tool: AgentTool,
    pid: u32,
    session_id: impl Into<String>,
    branch: Option<String>,
    is_worktree: bool,
    started_at: Option<chrono::DateTime<chrono::Local>>,
) -> Action {
    let session_id = session_id.into();
    let id = if tool == AgentTool::Codex && !session_id.is_empty() {
        AgentId::session(tool, session_id.clone())
    } else {
        AgentId::new(tool, pid)
    };

    Action::Detector(DetectorAction::SessionInfo {
        id,
        pid: Some(pid),
        session_id,
        branch,
        is_worktree,
        started_at,
    })
}

fn overview_detector_status_changed(
    tool: AgentTool,
    pid: u32,
    old: PrimaryStatus,
    new: PrimaryStatus,
) -> Action {
    Action::Detector(DetectorAction::StatusChanged {
        id: AgentId::new(tool, pid),
        pid: Some(pid),
        old,
        new,
    })
}

fn overview_detector_normalized_state_changed(
    tool: AgentTool,
    pid: u32,
    state: NormalizedState,
) -> Action {
    Action::Detector(DetectorAction::NormalizedStateChanged {
        id: AgentId::new(tool, pid),
        pid: Some(pid),
        state,
    })
}

fn overview_detector_process_exited(tool: AgentTool, pid: u32) -> Action {
    Action::Detector(DetectorAction::ProcessExited {
        id: AgentId::new(tool, pid),
        pid: Some(pid),
    })
}

fn reduce_overview(state: &mut RootState, action: Action) {
    let effects = reducer::root::reduce(state, &action);
    assert!(
        effects.is_empty(),
        "unexpected effects for {action:?}: {effects:?}"
    );
}

fn accounts_root_state() -> RootState {
    let mut state = RootState::default();
    state.accounts.entries = vec![
        AccountEntry {
            provider: Provider::Codex,
            profile_id: "codex-a".to_string(),
        },
        AccountEntry {
            provider: Provider::Codex,
            profile_id: "codex-b".to_string(),
        },
        AccountEntry {
            provider: Provider::Claude,
            profile_id: "claude-a".to_string(),
        },
    ];
    state.accounts.selected_index = Some(0);
    state
}

fn reduce_accounts(state: &mut RootState, action: Action) -> Vec<Effect> {
    reducer::root::reduce(state, &action)
}

#[test]
fn test_initial_state_is_empty() {
    let (_tmp, state) = make_state();
    assert!(state.agents.is_empty());
    assert_eq!(state.root.overview.selected_index, None);
    assert_eq!(state.root.overview.sort_mode, SortMode::Name);
    assert!(!state.show_help());
}

#[test]
fn overview_reducer_detector_updates_overview_agents() {
    let mut state = overview_root_state();
    let past = chrono::Local::now() - chrono::Duration::minutes(30);

    reduce_overview(
        &mut state,
        overview_detector_discovered(AgentTool::Claude, 41, "/tmp/overview", Some(past)),
    );
    reduce_overview(
        &mut state,
        overview_detector_session_info(
            AgentTool::Claude,
            41,
            "session-a",
            Some("feature/reducer".to_string()),
            true,
            None,
        ),
    );
    reduce_overview(
        &mut state,
        overview_detector_status_changed(
            AgentTool::Claude,
            41,
            PrimaryStatus::Running,
            PrimaryStatus::Waiting,
        ),
    );

    assert_eq!(state.overview.selected_index, Some(0));
    assert_eq!(state.overview.agents.len(), 1);
    assert_eq!(state.overview.visible_agents().len(), 1);
    assert_eq!(state.overview.visible_agents()[0].pid, Some(41));
    assert_eq!(
        state.overview.visible_agents()[0].session_id.as_deref(),
        Some("session-a")
    );
    assert_eq!(
        state.overview.visible_agents()[0].branch.as_deref(),
        Some("feature/reducer")
    );
    assert!(state.overview.visible_agents()[0].is_worktree);
    assert_eq!(
        state.overview.visible_agents()[0].status,
        PrimaryStatus::Waiting
    );
    assert_eq!(state.overview.visible_agents()[0].started_at, Some(past));
}

#[test]
fn accounts_reducer_selects_next_and_previous_with_provider_filter() {
    let mut state = accounts_root_state();

    let effects = reduce_accounts(
        &mut state,
        Action::Accounts(AccountsAction::SetProviderFilter(ProviderFilter::Claude)),
    );
    assert!(effects.is_empty());
    assert_eq!(state.accounts.provider_filter, ProviderFilter::Claude);
    assert_eq!(state.accounts.selected_index, Some(0));

    let effects = reduce_accounts(&mut state, Action::Accounts(AccountsAction::SelectNext));
    assert!(effects.is_empty());
    assert_eq!(state.accounts.selected_index, Some(0));

    let effects = reduce_accounts(&mut state, Action::Accounts(AccountsAction::SelectPrevious));
    assert!(effects.is_empty());
    assert_eq!(state.accounts.selected_index, Some(0));
}

#[test]
fn accounts_reducer_uses_explicit_filtering_and_editing_modes() {
    let mut state = accounts_root_state();

    let effects = reduce_accounts(&mut state, Action::Accounts(AccountsAction::StartFiltering));
    assert!(effects.is_empty());
    assert_eq!(
        state.accounts.mode,
        AccountsMode::Filtering {
            input: String::new()
        }
    );

    let effects = reduce_accounts(
        &mut state,
        Action::Accounts(AccountsAction::UpdateFilter("cod".to_string())),
    );
    assert!(effects.is_empty());
    assert_eq!(
        state.accounts.mode,
        AccountsMode::Filtering {
            input: "cod".to_string()
        }
    );

    let draft = ProfileDraft {
        label: "Profile".to_string(),
        email: Some("person@example.com".to_string()),
        plan: Some("pro".to_string()),
        notes: "notes".to_string(),
        enabled: true,
    };
    let effects = reduce_accounts(
        &mut state,
        Action::Accounts(AccountsAction::StartEditing(draft.clone())),
    );
    assert!(effects.is_empty());
    assert_eq!(
        state.accounts.mode,
        AccountsMode::Editing {
            draft,
            field: AccountsEditField::Label
        }
    );
}

#[test]
fn accounts_reducer_confirming_delete_emits_effect_and_sets_workflow() {
    let mut state = accounts_root_state();

    let effects = reduce_accounts(
        &mut state,
        Action::Accounts(AccountsAction::RequestConfirmation(
            AccountsConfirmAction::DeleteProfile,
        )),
    );
    assert!(effects.is_empty());
    assert_eq!(
        state.accounts.mode,
        AccountsMode::Confirming {
            action: AccountsConfirmAction::DeleteProfile
        }
    );

    let effects = reduce_accounts(&mut state, Action::Accounts(AccountsAction::Confirm));
    assert_eq!(
        effects,
        vec![Effect::DeleteProfile {
            provider: Provider::Codex,
            profile_id: "codex-a".to_string()
        }]
    );
    assert_eq!(state.accounts.mode, AccountsMode::Normal);
    assert_eq!(
        state.system.workflow,
        Workflow::DeletingProfile {
            provider: Provider::Codex,
            profile_id: "codex-a".to_string()
        }
    );
}

#[test]
fn accounts_reducer_confirming_edit_emits_save_effect_with_updated_draft() {
    let mut state = accounts_root_state();
    let draft = ProfileDraft {
        label: "Original".to_string(),
        email: Some("person@example.com".to_string()),
        plan: Some("pro".to_string()),
        notes: "old".to_string(),
        enabled: true,
    };
    reduce_accounts(
        &mut state,
        Action::Accounts(AccountsAction::StartEditing(draft)),
    );
    reduce_accounts(
        &mut state,
        Action::Accounts(AccountsAction::UpdateEditBuffer("Renamed".to_string())),
    );
    reduce_accounts(
        &mut state,
        Action::Accounts(AccountsAction::SetMode(AccountsMode::Editing {
            draft: ProfileDraft {
                label: "Renamed".to_string(),
                email: Some("person@example.com".to_string()),
                plan: Some("pro".to_string()),
                notes: "old".to_string(),
                enabled: true,
            },
            field: AccountsEditField::Notes,
        })),
    );
    reduce_accounts(
        &mut state,
        Action::Accounts(AccountsAction::UpdateEditBuffer("fresh notes".to_string())),
    );
    reduce_accounts(&mut state, Action::Accounts(AccountsAction::ToggleEnabled));

    let effects = reduce_accounts(&mut state, Action::Accounts(AccountsAction::Confirm));
    assert_eq!(
        effects,
        vec![Effect::SaveProfileEdits {
            provider: Provider::Codex,
            profile_id: "codex-a".to_string(),
            draft: ProfileDraft {
                label: "Renamed".to_string(),
                email: Some("person@example.com".to_string()),
                plan: Some("pro".to_string()),
                notes: "fresh notes".to_string(),
                enabled: false,
            }
        }]
    );
    assert_eq!(
        state.accounts.mode,
        AccountsMode::Editing {
            draft: ProfileDraft {
                label: "Renamed".to_string(),
                email: Some("person@example.com".to_string()),
                plan: Some("pro".to_string()),
                notes: "fresh notes".to_string(),
                enabled: false,
            },
            field: AccountsEditField::Notes,
        }
    );
    assert_eq!(
        state.system.workflow,
        Workflow::SavingProfile {
            provider: Provider::Codex,
            profile_id: "codex-a".to_string()
        }
    );
}

#[test]
fn accounts_reducer_failed_save_keeps_edit_draft_open() {
    let mut state = accounts_root_state();
    state.accounts.mode = AccountsMode::Editing {
        draft: ProfileDraft {
            label: "Renamed".to_string(),
            email: Some("person@example.com".to_string()),
            plan: Some("pro".to_string()),
            notes: "fresh notes".to_string(),
            enabled: false,
        },
        field: AccountsEditField::Notes,
    };
    state.system.workflow = Workflow::SavingProfile {
        provider: Provider::Codex,
        profile_id: "codex-a".to_string(),
    };

    let effects = reduce_accounts(
        &mut state,
        Action::EffectResult(EffectResult::Failed {
            effect: Effect::SaveProfileEdits {
                provider: Provider::Codex,
                profile_id: "codex-a".to_string(),
                draft: ProfileDraft {
                    label: "Renamed".to_string(),
                    email: Some("person@example.com".to_string()),
                    plan: Some("pro".to_string()),
                    notes: "fresh notes".to_string(),
                    enabled: false,
                },
            },
            message: "save failed".to_string(),
        }),
    );
    assert!(effects.is_empty());
    assert!(matches!(state.accounts.mode, AccountsMode::Editing { .. }));
    assert_eq!(state.system.workflow, Workflow::Idle);
    assert_eq!(
        state.accounts.status_message.as_deref(),
        Some("save failed")
    );
}

#[test]
fn accounts_reducer_profile_saved_closes_edit_mode() {
    let mut state = accounts_root_state();
    state.accounts.mode = AccountsMode::Editing {
        draft: ProfileDraft {
            label: "Renamed".to_string(),
            email: Some("person@example.com".to_string()),
            plan: Some("pro".to_string()),
            notes: "fresh notes".to_string(),
            enabled: false,
        },
        field: AccountsEditField::Notes,
    };
    state.system.workflow = Workflow::SavingProfile {
        provider: Provider::Codex,
        profile_id: "codex-a".to_string(),
    };

    let effects = reduce_accounts(
        &mut state,
        Action::EffectResult(EffectResult::ProfileSaved {
            profile: AccountProfile::Codex(nexus::domain::accounts::CodexProfile {
                id: "codex-a".to_string(),
                label: "Renamed".to_string(),
                email: Some("person@example.com".to_string()),
                plan: Some("pro".to_string()),
                notes: Some("fresh notes".to_string()),
                enabled: false,
                last_used: None,
                created_at: Utc::now(),
                updated_at: Utc::now(),
                profile_dir: PathBuf::from("/tmp/codex-a"),
                auth_path: PathBuf::from("/tmp/codex-a/auth.json"),
            }),
        }),
    );
    assert!(effects.is_empty());
    assert_eq!(state.accounts.mode, AccountsMode::Normal);
    assert_eq!(state.system.workflow, Workflow::Idle);
    assert_eq!(
        state.accounts.status_message.as_deref(),
        Some("Saved profile codex-a")
    );
}

#[test]
fn accounts_reducer_effect_results_update_profiles_quota_and_failures() {
    let mut state = accounts_root_state();
    let codex_profile = nexus::domain::accounts::CodexProfile {
        id: "codex-a".to_string(),
        label: "Codex A".to_string(),
        email: Some("codex@example.com".to_string()),
        plan: Some("pro".to_string()),
        notes: Some("notes".to_string()),
        enabled: true,
        last_used: None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        profile_dir: PathBuf::from("/tmp/codex-a"),
        auth_path: PathBuf::from("/tmp/codex-a/auth.json"),
    };
    let effects = reduce_accounts(
        &mut state,
        Action::EffectResult(EffectResult::ProfilesLoaded {
            profiles: vec![AccountProfile::Codex(codex_profile.clone())],
            entries: vec![AccountEntry {
                provider: Provider::Codex,
                profile_id: "codex-a".to_string(),
            }],
        }),
    );
    assert!(effects.is_empty());
    assert_eq!(state.accounts.entries.len(), 1);
    assert_eq!(state.accounts.profiles_by_key.len(), 1);

    let quota = quota_snapshot("fresh quota");
    let effects = reduce_accounts(
        &mut state,
        Action::EffectResult(EffectResult::CodexQuotaRefreshed {
            profile_id: "codex-a".to_string(),
            quota: quota.clone(),
            origin: nexus::app::RefreshOrigin::Manual,
        }),
    );
    assert!(effects.is_empty());
    assert_eq!(state.accounts.quota_by_key.len(), 1);
    assert_eq!(state.accounts.quota_by_key.values().next(), Some(&quota));

    let effects = reduce_accounts(
        &mut state,
        Action::EffectResult(EffectResult::Failed {
            effect: Effect::RefreshCodexQuota {
                profile_id: "codex-a".to_string(),
                origin: nexus::app::RefreshOrigin::Manual,
            },
            message: "network failed".to_string(),
        }),
    );
    assert!(effects.is_empty());
    assert_eq!(state.system.workflow, Workflow::Idle);
    assert_eq!(
        state.accounts.status_message.as_deref(),
        Some("network failed")
    );
}

#[test]
fn accounts_reducer_profile_draft_loaded_enters_edit_mode_for_selected_profile() {
    let mut state = accounts_root_state();

    let effects = reduce_accounts(
        &mut state,
        Action::EffectResult(EffectResult::ProfileDraftLoaded {
            provider: Provider::Claude,
            profile_id: "claude-a".to_string(),
            draft: ProfileDraft {
                label: "Claude A".to_string(),
                email: Some("claude@example.com".to_string()),
                plan: Some("max".to_string()),
                notes: "notes".to_string(),
                enabled: false,
            },
        }),
    );

    assert!(effects.is_empty());
    assert_eq!(
        state.accounts.selected_entry(),
        Some(&AccountEntry {
            provider: Provider::Claude,
            profile_id: "claude-a".to_string(),
        })
    );
    assert_eq!(
        state.accounts.mode,
        AccountsMode::Editing {
            draft: ProfileDraft {
                label: "Claude A".to_string(),
                email: Some("claude@example.com".to_string()),
                plan: Some("max".to_string()),
                notes: "notes".to_string(),
                enabled: false,
            },
            field: AccountsEditField::Label,
        }
    );
    assert_eq!(state.system.workflow, Workflow::Idle);
}

#[test]
fn accounts_reducer_imported_profile_becomes_selected_and_visible() {
    let mut state = accounts_root_state();
    state.accounts.entries = vec![AccountEntry {
        provider: Provider::Claude,
        profile_id: "claude-a".to_string(),
    }];
    state.accounts.provider_filter = ProviderFilter::Claude;
    state.accounts.selected_index = Some(0);

    let imported = AccountProfile::Codex(nexus::domain::accounts::CodexProfile {
        id: "codex-new".to_string(),
        label: "Codex New".to_string(),
        email: Some("codex@example.com".to_string()),
        plan: Some("pro".to_string()),
        notes: None,
        enabled: true,
        last_used: None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        profile_dir: PathBuf::from("/tmp/codex-new"),
        auth_path: PathBuf::from("/tmp/codex-new/auth.json"),
    });

    let effects = reduce_accounts(
        &mut state,
        Action::EffectResult(EffectResult::ProfileImported { profile: imported }),
    );

    assert!(effects.is_empty());
    assert_eq!(state.accounts.provider_filter, ProviderFilter::Codex);
    assert_eq!(
        state.accounts.selected_entry(),
        Some(&AccountEntry {
            provider: Provider::Codex,
            profile_id: "codex-new".to_string(),
        })
    );
}

#[test]
fn overview_reducer_codex_discovery_updates_all_ambiguous_live_session_rows() {
    let mut state = overview_root_state();

    reduce_overview(
        &mut state,
        Action::Detector(DetectorAction::SessionInfo {
            id: AgentId::session(AgentTool::Codex, "session-a"),
            pid: Some(44),
            session_id: "session-a".to_string(),
            branch: None,
            is_worktree: false,
            started_at: None,
        }),
    );
    reduce_overview(
        &mut state,
        Action::Detector(DetectorAction::SessionInfo {
            id: AgentId::session(AgentTool::Codex, "session-b"),
            pid: Some(44),
            session_id: "session-b".to_string(),
            branch: None,
            is_worktree: false,
            started_at: None,
        }),
    );

    let visible = state.overview.visible_agents();
    assert_eq!(visible.len(), 2);
    assert!(visible.iter().all(|agent| agent.cwd.as_os_str().is_empty()));

    reduce_overview(
        &mut state,
        overview_detector_discovered(AgentTool::Codex, 44, "/tmp/ambiguous", None),
    );

    let visible = state.overview.visible_agents();
    assert_eq!(visible.len(), 2);
    assert!(visible.iter().all(|agent| agent.id.is_session()));
    assert!(visible.iter().all(|agent| agent.pid == Some(44)));
    assert!(visible
        .iter()
        .all(|agent| agent.cwd == std::path::Path::new("/tmp/ambiguous")));
    assert!(visible
        .iter()
        .all(|agent| agent.status == PrimaryStatus::Running));
}

#[test]
fn overview_reducer_codex_process_exit_retires_all_session_rows_sharing_pid() {
    let mut state = overview_root_state();

    reduce_overview(
        &mut state,
        overview_detector_session_info(AgentTool::Codex, 44, "session-a", None, false, None),
    );
    reduce_overview(
        &mut state,
        overview_detector_session_info(AgentTool::Codex, 44, "session-b", None, false, None),
    );

    reduce_overview(
        &mut state,
        overview_detector_process_exited(AgentTool::Codex, 44),
    );

    let visible = state.overview.visible_agents();
    assert_eq!(visible.len(), 2);
    assert!(visible.iter().all(|agent| agent.id.is_session()));
    assert!(visible.iter().all(|agent| agent.pid == Some(44)));
    assert!(visible
        .iter()
        .all(|agent| agent.status == PrimaryStatus::Exited));
}

#[test]
fn overview_reducer_codex_session_info_absorbs_stale_process_row_after_pid_resolution() {
    let mut state = overview_root_state();

    reduce_overview(
        &mut state,
        Action::Detector(DetectorAction::Discovered {
            id: AgentId::session(AgentTool::Codex, "session-a"),
            pid: None,
            cwd: PathBuf::from("/tmp/ambiguous"),
            started_at: None,
        }),
    );
    reduce_overview(
        &mut state,
        overview_detector_discovered(AgentTool::Codex, 77, "/tmp/ambiguous", None),
    );

    assert_eq!(state.overview.agents.len(), 2);
    assert_eq!(
        state
            .overview
            .agents
            .iter()
            .filter(|agent| agent.id.is_session())
            .count(),
        1
    );
    assert_eq!(
        state
            .overview
            .agents
            .iter()
            .filter(|agent| agent.id.is_process())
            .count(),
        1
    );

    reduce_overview(
        &mut state,
        overview_detector_session_info(AgentTool::Codex, 77, "session-a", None, false, None),
    );

    assert_eq!(state.overview.agents.len(), 1);
    let agent = &state.overview.agents[0];
    assert!(agent.id.is_session());
    assert_eq!(agent.pid, Some(77));
    assert_eq!(agent.cwd, PathBuf::from("/tmp/ambiguous"));
    assert_eq!(agent.session_id.as_deref(), Some("session-a"));
    assert_eq!(state.overview.visible_agents().len(), 1);
}

#[test]
fn overview_reducer_codex_process_discovery_preserves_terminal_session_status() {
    let mut state = overview_root_state();

    reduce_overview(
        &mut state,
        overview_detector_session_info(AgentTool::Codex, 15642, "session-a", None, false, None),
    );
    reduce_overview(
        &mut state,
        Action::Detector(DetectorAction::StatusChanged {
            id: AgentId::session(AgentTool::Codex, "session-a"),
            pid: Some(15642),
            old: PrimaryStatus::Running,
            new: PrimaryStatus::Done,
        }),
    );

    reduce_overview(
        &mut state,
        overview_detector_discovered(AgentTool::Codex, 15642, "/tmp/project", None),
    );

    assert_eq!(state.overview.agents.len(), 1);
    let agent = &state.overview.agents[0];
    assert!(agent.id.is_session());
    assert_eq!(agent.pid, Some(15642));
    assert_eq!(agent.status, PrimaryStatus::Done);
    assert_eq!(agent.cwd, PathBuf::from("/tmp/project"));
}

#[test]
fn overview_reducer_codex_process_discovery_does_not_revive_exited_session_on_pid_reuse() {
    let mut state = overview_root_state();

    reduce_overview(
        &mut state,
        overview_detector_session_info(AgentTool::Codex, 44, "session-a", None, false, None),
    );
    reduce_overview(
        &mut state,
        overview_detector_process_exited(AgentTool::Codex, 44),
    );

    reduce_overview(
        &mut state,
        overview_detector_discovered(AgentTool::Codex, 44, "/tmp/reused", None),
    );

    assert_eq!(state.overview.agents.len(), 2);
    assert_eq!(
        state
            .overview
            .agents
            .iter()
            .filter(|agent| agent.id.is_session())
            .count(),
        1
    );
    assert_eq!(
        state
            .overview
            .agents
            .iter()
            .filter(|agent| agent.id.is_process())
            .count(),
        1
    );

    let exited_session = state
        .overview
        .agents
        .iter()
        .find(|agent| agent.id.is_session())
        .expect("exited session row");
    assert_eq!(exited_session.status, PrimaryStatus::Exited);
    assert_eq!(exited_session.pid, Some(44));

    let process_agent = state
        .overview
        .agents
        .iter()
        .find(|agent| agent.id.is_process())
        .expect("replacement process row");
    assert_eq!(process_agent.status, PrimaryStatus::Running);
    assert_eq!(process_agent.cwd, PathBuf::from("/tmp/reused"));
}

#[test]
fn overview_reducer_codex_process_discovery_revives_exited_session_with_same_pid_and_cwd() {
    let mut state = overview_root_state();

    reduce_overview(
        &mut state,
        Action::Detector(DetectorAction::Discovered {
            id: AgentId::session(AgentTool::Codex, "session-a"),
            pid: Some(44),
            cwd: PathBuf::from("/tmp/revive"),
            started_at: None,
        }),
    );
    reduce_overview(
        &mut state,
        overview_detector_session_info(AgentTool::Codex, 44, "session-a", None, false, None),
    );
    reduce_overview(
        &mut state,
        overview_detector_process_exited(AgentTool::Codex, 44),
    );

    reduce_overview(
        &mut state,
        overview_detector_discovered(AgentTool::Codex, 44, "/tmp/revive", None),
    );

    assert_eq!(state.overview.agents.len(), 1);
    let agent = &state.overview.agents[0];
    assert!(agent.id.is_session());
    assert_eq!(agent.pid, Some(44));
    assert_eq!(agent.status, PrimaryStatus::Running);
    assert_eq!(agent.cwd, PathBuf::from("/tmp/revive"));
}

#[test]
fn overview_reducer_codex_placeholder_session_transitions_preserve_pid_visibility() {
    let mut state = overview_root_state();

    reduce_overview(
        &mut state,
        Action::Detector(DetectorAction::SessionInfo {
            id: AgentId::session(AgentTool::Codex, "codex-a"),
            pid: None,
            session_id: "codex-a".to_string(),
            branch: Some("main".to_string()),
            is_worktree: false,
            started_at: None,
        }),
    );

    assert_eq!(state.overview.agents.len(), 1);
    assert!(state.overview.visible_agents().is_empty());
    assert_eq!(state.overview.selected_index, None);
    assert_eq!(
        state.overview.agents[0].normalized.status,
        NormalizedStatus::Unknown
    );

    reduce_overview(
        &mut state,
        overview_detector_session_info(
            AgentTool::Codex,
            42,
            "codex-a",
            Some("main".to_string()),
            true,
            None,
        ),
    );

    assert_eq!(state.overview.visible_agents().len(), 1);
    assert_eq!(state.overview.selected_index, Some(0));
    assert_eq!(state.overview.agents[0].pid, Some(42));
    assert_eq!(state.overview.agents[0].branch.as_deref(), Some("main"));
    assert!(state.overview.agents[0].is_worktree);

    reduce_overview(
        &mut state,
        Action::Detector(DetectorAction::SessionInfo {
            id: AgentId::session(AgentTool::Codex, "codex-a"),
            pid: None,
            session_id: "codex-a".to_string(),
            branch: Some("main".to_string()),
            is_worktree: false,
            started_at: None,
        }),
    );

    assert_eq!(state.overview.visible_agents().len(), 1);
    assert_eq!(state.overview.agents[0].pid, Some(42));
}

#[test]
fn overview_reducer_rename_start_confirm_and_cancel_are_pure_state_transitions() {
    let mut state = overview_root_state();
    reduce_overview(
        &mut state,
        overview_detector_discovered(AgentTool::Claude, 42, "/tmp/rename", None),
    );

    let agent_id = state.overview.agents[0].id.clone();
    reduce_overview(
        &mut state,
        Action::Overview(OverviewAction::StartRenaming {
            agent_id: agent_id.clone(),
        }),
    );
    assert_eq!(
        state.overview.mode,
        OverviewMode::Renaming {
            agent_id: agent_id.clone(),
            input: String::new(),
        }
    );

    reduce_overview(
        &mut state,
        Action::Overview(OverviewAction::UpdateRenameInput(
            "renamed-claude".to_string(),
        )),
    );
    reduce_overview(&mut state, Action::Overview(OverviewAction::ConfirmRename));

    assert_eq!(state.overview.mode, OverviewMode::Normal);
    assert_eq!(
        state.overview.agents[0].alias.as_deref(),
        Some("renamed-claude")
    );

    reduce_overview(
        &mut state,
        Action::Overview(OverviewAction::StartRenaming {
            agent_id: agent_id.clone(),
        }),
    );
    reduce_overview(
        &mut state,
        Action::Overview(OverviewAction::UpdateRenameInput("discard-me".to_string())),
    );
    reduce_overview(&mut state, Action::Overview(OverviewAction::CancelMode));

    assert_eq!(state.overview.mode, OverviewMode::Normal);
    assert_eq!(
        state.overview.agents[0].alias.as_deref(),
        Some("renamed-claude")
    );
}

#[test]
fn overview_reducer_filter_and_sort_transitions_update_visible_order() {
    let mut state = overview_root_state();
    reduce_overview(
        &mut state,
        overview_detector_discovered(AgentTool::OpenCode, 1, "/tmp/opencode", None),
    );
    reduce_overview(
        &mut state,
        overview_detector_discovered(AgentTool::Claude, 2, "/tmp/claude", None),
    );
    reduce_overview(
        &mut state,
        overview_detector_discovered(AgentTool::Codex, 3, "/tmp/codex", None),
    );
    reduce_overview(
        &mut state,
        overview_detector_normalized_state_changed(
            AgentTool::OpenCode,
            1,
            NormalizedState::new(
                NormalizedStatus::Done,
                Some(NormalizedReason::ReadyForNextTask),
            ),
        ),
    );
    reduce_overview(
        &mut state,
        overview_detector_normalized_state_changed(
            AgentTool::Claude,
            2,
            NormalizedState::new(NormalizedStatus::NeedsInput, Some(NormalizedReason::Review)),
        ),
    );
    reduce_overview(
        &mut state,
        overview_detector_normalized_state_changed(
            AgentTool::Codex,
            3,
            NormalizedState::new(NormalizedStatus::Blocked, Some(NormalizedReason::Error)),
        ),
    );

    reduce_overview(&mut state, Action::Overview(OverviewAction::StartFiltering));
    reduce_overview(
        &mut state,
        Action::Overview(OverviewAction::UpdateFilter("claude".to_string())),
    );
    assert_eq!(
        state.overview.mode,
        OverviewMode::Filtering {
            input: "claude".to_string(),
        }
    );
    assert_eq!(state.overview.filter.as_deref(), Some("claude"));
    assert_eq!(state.overview.visible_agents().len(), 1);
    assert_eq!(state.overview.selected_index, Some(0));

    reduce_overview(&mut state, Action::Overview(OverviewAction::ConfirmFilter));
    assert_eq!(state.overview.mode, OverviewMode::Normal);

    reduce_overview(&mut state, Action::Overview(OverviewAction::ClearFilter));
    reduce_overview(&mut state, Action::Overview(OverviewAction::CycleSortMode));
    reduce_overview(&mut state, Action::Overview(OverviewAction::CycleSortMode));

    let visible = state.overview.visible_agents();
    assert_eq!(state.overview.sort_mode, SortMode::Urgency);
    assert_eq!(visible[0].pid, Some(2));
    assert_eq!(visible[1].pid, Some(3));
    assert_eq!(visible[2].pid, Some(1));
}

#[test]
fn overview_reducer_dismiss_actions_remove_exited_and_selected_agents() {
    let mut state = overview_root_state();
    reduce_overview(
        &mut state,
        overview_detector_discovered(AgentTool::Claude, 11, "/tmp/one", None),
    );
    reduce_overview(
        &mut state,
        overview_detector_discovered(AgentTool::OpenCode, 12, "/tmp/two", None),
    );
    reduce_overview(
        &mut state,
        overview_detector_process_exited(AgentTool::Claude, 11),
    );

    assert_eq!(state.overview.agents.len(), 1);
    assert_eq!(state.overview.visible_agents().len(), 1);

    reduce_overview(&mut state, Action::Overview(OverviewAction::DismissExited));
    assert_eq!(state.overview.agents.len(), 1);
    assert_eq!(state.overview.visible_agents()[0].pid, Some(12));
    assert_eq!(state.overview.selected_index, Some(0));

    reduce_overview(
        &mut state,
        Action::Overview(OverviewAction::DismissSelected),
    );
    assert!(state.overview.agents.is_empty());
    assert!(state.overview.visible_agents().is_empty());
    assert_eq!(state.overview.selected_index, None);
}

#[test]
fn overview_reducer_tracks_normalized_status_inputs_for_header_aggregation() {
    let mut state = overview_root_state();
    reduce_overview(
        &mut state,
        overview_detector_discovered(AgentTool::Codex, 21, "/tmp/codex", None),
    );
    reduce_overview(
        &mut state,
        overview_detector_discovered(AgentTool::Claude, 22, "/tmp/claude", None),
    );
    reduce_overview(
        &mut state,
        overview_detector_normalized_state_changed(
            AgentTool::Claude,
            22,
            NormalizedState::new(
                NormalizedStatus::NeedsInput,
                Some(NormalizedReason::Approval),
            ),
        ),
    );

    assert_eq!(
        state.overview.normalized_status_counts(),
        nexus::app::OverviewStatusCounts {
            total: 2,
            running: 0,
            waiting: 1,
            error: 1,
            done: 0,
        }
    );
}

#[test]
fn test_initial_state_starts_on_overview_tab() {
    let (_tmp, state) = make_state();
    assert_eq!(state.active_tab(), ActiveTab::Overview);
}

#[test]
fn test_tab_navigation_cycles_between_tabs() {
    let (_tmp, mut state) = make_state();
    assert_eq!(state.active_tab(), ActiveTab::Overview);

    state.next_tab();
    assert_eq!(state.active_tab(), ActiveTab::Accounts);

    state.previous_tab();
    assert_eq!(state.active_tab(), ActiveTab::Overview);

    state.previous_tab();
    assert_eq!(state.active_tab(), ActiveTab::Accounts);

    state.next_tab();
    assert_eq!(state.active_tab(), ActiveTab::Overview);
}

#[tokio::test]
async fn codex_tab_navigation_keys_do_not_move_overview_selection() {
    let (_tmp, mut state) = make_state();
    state.next_tab();
    assert_eq!(state.active_tab(), ActiveTab::Accounts);

    state.root.accounts.entries = vec![
        AccountEntry {
            provider: Provider::Codex,
            profile_id: "one".to_string(),
        },
        AccountEntry {
            provider: Provider::Codex,
            profile_id: "two".to_string(),
        },
    ];
    state.apply_accounts_effect_result(EffectResult::ProfilesLoaded {
        profiles: Vec::new(),
        entries: state.root.accounts.entries.clone(),
    });
    state
        .run_accounts_reducer_action(AccountsAction::SelectNext)
        .await;

    assert_eq!(state.root.accounts.selected_index, Some(1));
    assert_eq!(state.root.overview.selected_index, None);
}

#[tokio::test]
async fn codex_tab_filter_input_stays_on_codex_state() {
    let (_tmp, mut state) = make_state();
    state.next_tab();
    assert_eq!(state.active_tab(), ActiveTab::Accounts);

    state
        .run_accounts_reducer_action(AccountsAction::StartFiltering)
        .await;
    state
        .run_accounts_reducer_action(AccountsAction::UpdateFilter("q1".to_string()))
        .await;

    assert!(state.root.overview.filter.is_none());
    assert_eq!(
        state.root.accounts.mode,
        AccountsMode::Filtering {
            input: "q1".to_string()
        }
    );

    state
        .run_accounts_reducer_action(AccountsAction::Confirm)
        .await;
    assert_eq!(state.root.accounts.mode, AccountsMode::Normal);
}

#[test]
fn codex_footer_uses_codex_shortcuts() {
    let (_tmp, mut state) = make_state();
    state.next_tab();

    let footer = footer::shortcut_text(&state);
    assert!(footer.contains("import codex"));
    assert!(footer.contains("import claude"));
    assert!(footer.contains("R refresh"));
    assert!(footer.contains("? help"));
    assert!(!footer.contains("sort:"));
    assert!(!footer.contains(" rename"));
}

#[test]
fn help_overlay_lists_import_and_management_shortcuts() {
    let lines = help::help_lines(false);
    let content: Vec<String> = lines
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect()
        })
        .collect();

    assert!(content
        .iter()
        .any(|line| line.contains("Import ~/.codex/auth.json")));
    assert!(content
        .iter()
        .any(|line| line.contains("Import Claude credentials")));
    assert!(content.iter().any(|line| line.contains("Refresh quota")));
    assert!(content.iter().any(|line| line.contains("Toggle help")));
    assert!(content.iter().any(|line| line.contains("Accounts")));
}

#[test]
fn overview_empty_state_is_generic_when_detector_health_is_nominal() {
    let (_tmp, mut state) = make_state();
    state.started_at = std::time::Instant::now()
        - std::time::Duration::from_secs(state.config.poll_interval_secs + 1);

    assert_eq!(
        agent_list::empty_state_message(&state),
        "No agents detected. Start Claude, Codex, or OpenCode to see them here."
    );
}

#[test]
fn overview_empty_state_surfaces_detector_dependency_failure() {
    let (_tmp, mut state) = make_state();
    state.started_at = std::time::Instant::now()
        - std::time::Duration::from_secs(state.config.poll_interval_secs + 1);

    state.handle_event(AgentEvent::DetectorHealthChanged {
        detector: "process scanner".to_string(),
        healthy: false,
        message:
            "Process scanner degraded: `lsof` is unavailable; install it to enable agent detection."
                .to_string(),
    });

    assert_eq!(
        agent_list::empty_state_message(&state),
        "Process scanner degraded: `lsof` is unavailable; install it to enable agent detection."
    );
}

#[tokio::test]
async fn accounts_filter_confirm_keeps_text_until_dismissed() {
    let (_tmp, mut state) = make_state();
    state.next_tab();

    state
        .run_accounts_reducer_action(AccountsAction::StartFiltering)
        .await;
    state
        .run_accounts_reducer_action(AccountsAction::UpdateFilter("q1".to_string()))
        .await;
    state
        .run_accounts_reducer_action(AccountsAction::Confirm)
        .await;

    assert_eq!(state.root.accounts.mode, AccountsMode::Normal);

    state.root.accounts.mode = AccountsMode::Editing {
        draft: ProfileDraft {
            label: "Temp".to_string(),
            email: None,
            plan: None,
            notes: String::new(),
            enabled: true,
        },
        field: AccountsEditField::Label,
    };

    state.dismiss_accounts_overlay();

    assert_eq!(state.root.accounts.mode, AccountsMode::Normal);
}

#[tokio::test]
async fn accounts_edit_starts_with_selected_label_and_cancels_cleanly() {
    let (_tmp, mut state) = make_state();
    state.next_tab();
    state.root.accounts.entries = vec![AccountEntry {
        provider: Provider::Codex,
        profile_id: "profile-01".to_string(),
    }];
    state.root.accounts.selected_index = Some(0);
    state.apply_accounts_effect_result(EffectResult::ProfilesLoaded {
        profiles: Vec::new(),
        entries: state.root.accounts.entries.clone(),
    });

    state
        .run_accounts_effect(Effect::LoadProfileDraft {
            provider: Provider::Codex,
            profile_id: "profile-01".to_string(),
        })
        .await;
    match &state.root.accounts.mode {
        AccountsMode::Editing { draft, field } => {
            assert_eq!(draft.label, "profile-01");
            assert_eq!(*field, AccountsEditField::Label);
        }
        mode => panic!("expected editing mode, got {mode:?}"),
    }

    state.root.accounts.mode = AccountsMode::Normal;
    assert_eq!(state.root.accounts.mode, AccountsMode::Normal);
}

#[tokio::test]
async fn accounts_edit_uses_saved_metadata_when_codex_auth_json_is_invalid() {
    let (_tmp, mut state) = make_state();
    state.next_tab();
    let profile_dir = state.config.codex_profiles_dir.join("profile-01");
    std::fs::create_dir_all(&profile_dir).unwrap();
    std::fs::write(profile_dir.join("auth.json"), "{not-json").unwrap();
    std::fs::write(
        profile_dir.join("profile.json"),
        r#"{
            "id":"profile-01",
            "label":"Saved Label",
            "notes":"saved notes",
            "enabled":false
        }"#,
    )
    .unwrap();
    state.root.accounts.entries = vec![AccountEntry {
        provider: Provider::Codex,
        profile_id: "profile-01".to_string(),
    }];
    state.root.accounts.selected_index = Some(0);
    state.apply_accounts_effect_result(EffectResult::ProfilesLoaded {
        profiles: Vec::new(),
        entries: state.root.accounts.entries.clone(),
    });

    state
        .run_accounts_effect(Effect::LoadProfileDraft {
            provider: Provider::Codex,
            profile_id: "profile-01".to_string(),
        })
        .await;

    match &state.root.accounts.mode {
        AccountsMode::Editing { draft, .. } => {
            assert_eq!(draft.label, "Saved Label");
            assert_eq!(draft.notes, "saved notes");
            assert!(!draft.enabled);
        }
        mode => panic!("expected editing mode, got {mode:?}"),
    }
}

#[tokio::test]
async fn accounts_edit_can_rewrite_corrupted_codex_profile_metadata() {
    let (_tmp, mut state) = make_state();
    state.next_tab();
    let profile_dir = state.config.codex_profiles_dir.join("profile-02");
    std::fs::create_dir_all(&profile_dir).unwrap();
    std::fs::write(
        profile_dir.join("auth.json"),
        r#"{"email":"current@example.com","plan":"team"}"#,
    )
    .unwrap();
    std::fs::write(profile_dir.join("profile.json"), "{broken-json").unwrap();
    state.root.accounts.entries = vec![AccountEntry {
        provider: Provider::Codex,
        profile_id: "profile-02".to_string(),
    }];
    state.root.accounts.selected_index = Some(0);
    state.apply_accounts_effect_result(EffectResult::ProfilesLoaded {
        profiles: Vec::new(),
        entries: state.root.accounts.entries.clone(),
    });

    state
        .run_accounts_effect(Effect::LoadProfileDraft {
            provider: Provider::Codex,
            profile_id: "profile-02".to_string(),
        })
        .await;
    state
        .run_accounts_reducer_action(AccountsAction::SetMode(AccountsMode::Editing {
            draft: ProfileDraft {
                label: "Recovered Profile".to_string(),
                email: Some("current@example.com".to_string()),
                plan: Some("team".to_string()),
                notes: "fixed notes".to_string(),
                enabled: false,
            },
            field: AccountsEditField::Label,
        }))
        .await;
    state
        .run_accounts_reducer_action(AccountsAction::Confirm)
        .await;

    let saved = nexus::codex_profiles::profile::load_profile_metadata(&profile_dir).unwrap();
    assert_eq!(saved.label.as_deref(), Some("Recovered Profile"));
    assert_eq!(saved.notes.as_deref(), Some("fixed notes"));
    assert!(!saved.enabled);
}

#[test]
fn quota_refresh_updates_selected_profile_and_cache_only() {
    let (_tmp, mut state) = make_state();
    state.next_tab();
    state.root.accounts.entries = vec![
        AccountEntry {
            provider: Provider::Codex,
            profile_id: "codex-a".to_string(),
        },
        AccountEntry {
            provider: Provider::Codex,
            profile_id: "codex-b".to_string(),
        },
    ];
    state.root.accounts.selected_index = Some(0);
    state.root.accounts.quota_loading = true;

    let existing = quota_snapshot("existing");
    state.root.accounts.quota_by_key.insert(
        nexus::app::state::AccountKey::new(Provider::Codex, "codex-a".to_string()),
        existing.clone(),
    );
    let refreshed = quota_snapshot("refreshed");

    state.apply_accounts_effect_result(EffectResult::CodexQuotaRefreshed {
        profile_id: "codex-b".to_string(),
        quota: refreshed.clone(),
        origin: nexus::app::RefreshOrigin::Manual,
    });

    assert!(!state.root.accounts.quota_loading);
    assert_eq!(
        state.root.accounts.quota_by_key.get(&nexus::app::state::AccountKey::new(
            Provider::Codex,
            "codex-b".to_string()
        )),
        Some(&refreshed)
    );
    assert_eq!(
        state.root.accounts.quota_by_key.get(&nexus::app::state::AccountKey::new(
            Provider::Codex,
            "codex-a".to_string()
        )),
        Some(&existing)
    );
}

#[test]
fn quota_refresh_updates_selected_claude_profile_and_cache() {
    let (_tmp, mut state) = make_state();
    state.next_tab();
    state.root.accounts.entries = vec![AccountEntry {
        provider: Provider::Claude,
        profile_id: "claude-a".to_string(),
    }];
    state.root.accounts.selected_index = Some(0);
    state.root.accounts.quota_loading = true;

    let refreshed = quota_snapshot("claude refreshed");
    state.apply_accounts_effect_result(EffectResult::ClaudeQuotaRefreshed {
        profile_id: "claude-a".to_string(),
        quota: refreshed.clone(),
        origin: nexus::app::RefreshOrigin::Manual,
    });

    assert!(!state.root.accounts.quota_loading);
    assert_eq!(
        state.root.accounts.quota_by_key.get(&nexus::app::state::AccountKey::new(
            Provider::Claude,
            "claude-a".to_string()
        )),
        Some(&refreshed)
    );
}

#[test]
fn manual_quota_refresh_failure_clears_loading_and_preserves_existing_data() {
    let (_tmp, mut state) = make_state();
    state.next_tab();
    state.root.accounts.entries = vec![AccountEntry {
        provider: Provider::Codex,
        profile_id: "codex-a".to_string(),
    }];
    state.root.accounts.selected_index = Some(0);
    state.begin_manual_quota_refresh(Provider::Codex, "codex-a");

    let cached = quota_snapshot("cached");
    state.root.accounts.quota_by_key.insert(
        nexus::app::state::AccountKey::new(Provider::Codex, "codex-a".to_string()),
        cached.clone(),
    );

    state.apply_accounts_effect_result(EffectResult::Failed {
        effect: Effect::RefreshCodexQuota {
            profile_id: "codex-a".to_string(),
            origin: nexus::app::RefreshOrigin::Manual,
        },
        message: "manual refresh failed".to_string(),
    });

    assert!(!state.root.accounts.quota_loading);
    assert_eq!(
        state.root.accounts.status_message.as_deref(),
        Some("manual refresh failed")
    );
    assert_eq!(
        state.root.accounts.quota_by_key.get(&nexus::app::state::AccountKey::new(
            Provider::Codex,
            "codex-a".to_string()
        )),
        Some(&cached)
    );
}

#[test]
fn background_quota_refresh_failure_does_not_set_status_message() {
    let (_tmp, mut state) = make_state();
    state.next_tab();
    state.apply_accounts_effect_result(EffectResult::Failed {
        effect: Effect::RefreshCodexQuota {
            profile_id: "codex-a".to_string(),
            origin: nexus::app::RefreshOrigin::Background,
        },
        message: "background refresh failed".to_string(),
    });

    assert_eq!(state.root.accounts.status_message, None);
}

#[test]
fn background_refresh_failure_does_not_cancel_manual_refresh_in_flight() {
    let (_tmp, mut state) = make_state();
    state.next_tab();
    state.begin_manual_quota_refresh(Provider::Claude, "claude-a");

    state.apply_accounts_effect_result(EffectResult::Failed {
        effect: Effect::RefreshCodexQuota {
            profile_id: "codex-a".to_string(),
            origin: nexus::app::RefreshOrigin::Background,
        },
        message: "background refresh failed".to_string(),
    });

    assert!(state.root.accounts.quota_loading);
    assert_eq!(state.root.accounts.status_message, None);
}

#[test]
fn background_refresh_success_for_same_profile_does_not_finish_manual_refresh() {
    let (_tmp, mut state) = make_state();
    state.next_tab();
    state.begin_manual_quota_refresh(Provider::Claude, "claude-a");

    state.apply_accounts_effect_result(EffectResult::ClaudeQuotaRefreshed {
        profile_id: "claude-a".to_string(),
        quota: quota_snapshot("background still loading"),
        origin: nexus::app::RefreshOrigin::Background,
    });

    assert!(state.root.accounts.quota_loading);
}

#[test]
fn app_state_new_does_not_eagerly_reload_profile_lists() {
    let temp = tempdir().expect("tempdir");
    let root = temp.path().to_path_buf();

    let mut config = Config::default();
    config.codex_management_dir = root.join("codex-management");
    config.codex_profiles_dir = config.codex_management_dir.join("profiles");
    config.claude_management_dir = root.join("claude-management");
    config.claude_profiles_dir = config.claude_management_dir.join("profiles");
    std::fs::create_dir_all(config.codex_profiles_dir.join("profile-01")).unwrap();
    std::fs::write(
        config
            .codex_profiles_dir
            .join("profile-01")
            .join("auth.json"),
        "{}",
    )
    .unwrap();
    std::fs::create_dir_all(config.claude_profiles_dir.join("profile-02")).unwrap();
    std::fs::write(
        config
            .claude_profiles_dir
            .join("profile-02")
            .join(".credentials.json"),
        r#"{"claudeAiOauth":{"accessToken":"tok","refreshToken":"ref","expiresAt":9999999999000}}"#,
    )
    .unwrap();

    let state = AppState::new(config);

    assert!(state.root.accounts.entries.is_empty());
}

#[tokio::test]
async fn initialize_accounts_loads_profiles_via_dispatcher() {
    let temp = tempdir().expect("tempdir");
    let root = temp.path().to_path_buf();

    let mut config = Config::default();
    config.codex_management_dir = root.join("codex-management");
    config.codex_profiles_dir = config.codex_management_dir.join("profiles");
    config.claude_management_dir = root.join("claude-management");
    config.claude_profiles_dir = config.claude_management_dir.join("profiles");
    std::fs::create_dir_all(config.codex_profiles_dir.join("profile-01")).unwrap();
    std::fs::write(
        config
            .codex_profiles_dir
            .join("profile-01")
            .join("auth.json"),
        r#"{"email":"dev@example.com","plan":"pro"}"#,
    )
    .unwrap();
    std::fs::create_dir_all(config.claude_profiles_dir.join("profile-02")).unwrap();
    std::fs::write(
        config
            .claude_profiles_dir
            .join("profile-02")
            .join(".credentials.json"),
        r#"{"claudeAiOauth":{"accessToken":"tok","refreshToken":"ref","expiresAt":9999999999000,"subscriptionType":"claude_max"}}"#,
    )
    .unwrap();

    let mut state = AppState::new(config);
    state.initialize_accounts().await;

    assert_eq!(
        state.root.accounts.entries,
        vec![
            AccountEntry {
                provider: Provider::Codex,
                profile_id: "profile-01".to_string(),
            },
            AccountEntry {
                provider: Provider::Claude,
                profile_id: "profile-02".to_string(),
            },
        ]
    );
}

#[tokio::test]
async fn initialize_accounts_skips_invalid_profile_and_loads_remaining_profiles() {
    let temp = tempdir().expect("tempdir");
    let root = temp.path().to_path_buf();

    let mut config = Config::default();
    config.codex_management_dir = root.join("codex-management");
    config.codex_profiles_dir = config.codex_management_dir.join("profiles");
    config.claude_management_dir = root.join("claude-management");
    config.claude_profiles_dir = config.claude_management_dir.join("profiles");

    std::fs::create_dir_all(config.codex_profiles_dir.join("profile-01")).unwrap();
    std::fs::write(
        config
            .codex_profiles_dir
            .join("profile-01")
            .join("auth.json"),
        r#"{"email":"dev@example.com","plan":"pro"}"#,
    )
    .unwrap();

    std::fs::create_dir_all(config.claude_profiles_dir.join("profile-02")).unwrap();
    std::fs::write(
        config
            .claude_profiles_dir
            .join("profile-02")
            .join(".credentials.json"),
        r#"{"claudeAiOauth":{"accessToken":"tok","refreshToken":"ref","expiresAt":9999999999000}}"#,
    )
    .unwrap();
    std::fs::write(
        config
            .claude_profiles_dir
            .join("profile-02")
            .join("profile.json"),
        "{broken-json",
    )
    .unwrap();

    let mut state = AppState::new(config);
    state.initialize_accounts().await;

    assert_eq!(
        state.root.accounts.entries,
        vec![AccountEntry {
            provider: Provider::Codex,
            profile_id: "profile-01".to_string(),
        }]
    );
}

#[tokio::test]
async fn accounts_reducer_updates_root_accounts_mode_and_filter_state() {
    let (_tmp, mut state) = make_state();

    assert_eq!(state.root.accounts.mode, AccountsMode::Normal);
    assert_eq!(state.root.accounts.provider_filter, ProviderFilter::All);

    state
        .run_accounts_reducer_action(AccountsAction::StartFiltering)
        .await;
    assert_eq!(
        state.root.accounts.mode,
        AccountsMode::Filtering {
            input: String::new()
        }
    );

    state
        .run_accounts_reducer_action(AccountsAction::UpdateFilter("cod".to_string()))
        .await;
    assert_eq!(
        state.root.accounts.mode,
        AccountsMode::Filtering {
            input: "cod".to_string()
        }
    );

    state
        .run_accounts_reducer_action(AccountsAction::SetProviderFilter(ProviderFilter::Claude))
        .await;
    assert_eq!(state.root.accounts.provider_filter, ProviderFilter::Claude);
}

#[tokio::test]
async fn accounts_reducer_updates_root_accounts_selection() {
    let (_tmp, mut state) = make_state();
    state.root.accounts.entries = vec![
        AccountEntry {
            provider: Provider::Codex,
            profile_id: "codex-a".to_string(),
        },
        AccountEntry {
            provider: Provider::Claude,
            profile_id: "claude-a".to_string(),
        },
    ];
    state
        .run_accounts_reducer_action(AccountsAction::SelectNext)
        .await;
    assert_eq!(state.root.accounts.selected_index, Some(0));

    state
        .run_accounts_reducer_action(AccountsAction::SelectNext)
        .await;
    assert_eq!(state.root.accounts.selected_index, Some(1));
}

#[test]
fn quota_refresh_persist_failure_keeps_updated_cache_in_memory() {
    let (_tmp, mut state) = make_state();
    state.next_tab();
    state.root.accounts.entries = vec![AccountEntry {
        provider: Provider::Codex,
        profile_id: "codex-a".to_string(),
    }];
    state.root.accounts.selected_index = Some(0);
    state.root.accounts.quota_loading = true;
    std::fs::create_dir_all(state.config.codex_management_dir.join("quota-cache-dir")).unwrap();
    state.config.codex_quota_cache_file = state.config.codex_management_dir.join("quota-cache-dir");

    let refreshed = quota_snapshot("persist failure still cached");
    state.apply_accounts_effect_result(EffectResult::CodexQuotaRefreshed {
        profile_id: "codex-a".to_string(),
        quota: refreshed.clone(),
        origin: nexus::app::RefreshOrigin::Manual,
    });

    assert!(!state.root.accounts.quota_loading);
    assert_eq!(
        state.root.accounts.quota_by_key.get(&nexus::app::state::AccountKey::new(
            Provider::Codex,
            "codex-a".to_string()
        )),
        Some(&refreshed)
    );
}

#[test]
fn quota_refresh_effect_result_persists_cache_to_disk() {
    let (_tmp, mut state) = make_state();
    state.next_tab();
    state.root.accounts.entries = vec![AccountEntry {
        provider: Provider::Codex,
        profile_id: "codex-a".to_string(),
    }];
    state.root.accounts.selected_index = Some(0);

    let refreshed = quota_snapshot("persisted from effect result");
    state.apply_accounts_effect_result(EffectResult::CodexQuotaRefreshed {
        profile_id: "codex-a".to_string(),
        quota: refreshed.clone(),
        origin: nexus::app::RefreshOrigin::Manual,
    });

    let saved =
        nexus::services::accounts::quota::load_quota_cache(&state.config.codex_quota_cache_file)
            .expect("quota cache should be written");
    assert_eq!(saved.by_profile.get("codex-a"), Some(&refreshed));
}

#[test]
fn deleting_active_codex_profile_clears_active_badge() {
    let (_tmp, mut state) = make_state();
    state.next_tab();
    state.root.accounts.entries = vec![AccountEntry {
        provider: Provider::Codex,
        profile_id: "profile-01".to_string(),
    }];
    state.root.accounts.selected_index = Some(0);
    state.set_active_codex_profile(Some("profile-01".to_string()));

    state.apply_accounts_effect_result(EffectResult::ProfileDeleted {
        entry: AccountEntry {
            provider: Provider::Codex,
            profile_id: "profile-01".to_string(),
        },
    });

    assert_eq!(state.active_codex_profile_id(), None);
}

#[test]
fn deleting_active_claude_profile_clears_active_badge() {
    let (_tmp, mut state) = make_state();
    state.next_tab();
    state.root.accounts.entries = vec![AccountEntry {
        provider: Provider::Claude,
        profile_id: "profile-02".to_string(),
    }];
    state.root.accounts.provider_filter = ProviderFilter::Claude;
    state.root.accounts.selected_index = Some(0);
    state.root.accounts.active_claude_profile_id = Some("profile-02".to_string());

    state.apply_accounts_effect_result(EffectResult::ProfileDeleted {
        entry: AccountEntry {
            provider: Provider::Claude,
            profile_id: "profile-02".to_string(),
        },
    });

    assert_eq!(state.root.accounts.active_claude_profile_id, None);
}

#[test]
fn test_discover_agent() {
    let (_tmp, mut state) = make_state();
    state.handle_event(discovered(
        AgentTool::Claude,
        1234,
        PathBuf::from("/tmp/project"),
        None,
    ));
    assert_eq!(state.agents.len(), 1);
    assert_eq!(state.agents[0].pid, Some(1234));
    assert_eq!(state.agents[0].id.tool(), AgentTool::Claude);
    assert_eq!(state.agents[0].status, PrimaryStatus::Running);
    assert_eq!(state.root.overview.selected_index, Some(0));
}

#[test]
fn test_discover_codex_starts_unknown_not_working() {
    let (_tmp, mut state) = make_state();
    state.handle_event(discovered(
        AgentTool::Codex,
        1234,
        PathBuf::from("/tmp/project"),
        None,
    ));

    assert_eq!(state.agents.len(), 1);
    assert_eq!(state.agents[0].status, PrimaryStatus::Running);
    assert_eq!(state.agents[0].normalized.status, NormalizedStatus::Unknown);
    assert_eq!(state.agents[0].normalized.reason, None);
}

#[test]
fn test_discover_duplicate_ignored() {
    let (_tmp, mut state) = make_state();
    state.handle_event(discovered(
        AgentTool::Claude,
        1234,
        PathBuf::from("/tmp/project"),
        None,
    ));
    state.handle_event(discovered(
        AgentTool::Claude,
        1234,
        PathBuf::from("/tmp/project"),
        None,
    ));
    assert_eq!(state.agents.len(), 1);
}

#[test]
fn test_status_change() {
    let (_tmp, mut state) = make_state();
    state.handle_event(discovered(
        AgentTool::OpenCode,
        5678,
        PathBuf::from("/tmp/oc"),
        None,
    ));
    state.handle_event(status_changed(
        AgentTool::OpenCode,
        5678,
        PrimaryStatus::Running,
        PrimaryStatus::Waiting,
    ));
    assert_eq!(state.agents[0].status, PrimaryStatus::Waiting);
}

#[test]
fn test_process_exited() {
    let (_tmp, mut state) = make_state();
    state.handle_event(discovered(
        AgentTool::Claude,
        9999,
        PathBuf::from("/tmp/cx"),
        None,
    ));
    state.handle_event(process_exited(AgentTool::Claude, 9999));
    assert!(state.agents.is_empty());
}

#[test]
fn test_subagent_spawned_and_completed() {
    let (_tmp, mut state) = make_state();
    state.handle_event(discovered(
        AgentTool::OpenCode,
        1000,
        PathBuf::from("/tmp"),
        None,
    ));
    state.handle_event(subagent_spawned(
        AgentTool::OpenCode,
        1000,
        Subagent {
            name: "explore".to_string(),
            status: PrimaryStatus::Running,
            reason: None,
            description: Some("searching codebase".to_string()),
            started_at: chrono::Local::now(),
            ended_at: None,
        },
    ));
    assert_eq!(state.agents[0].subagents.len(), 1);
    assert_eq!(state.agents[0].subagents[0].name, "explore");
    state.handle_event(subagent_completed(
        AgentTool::OpenCode,
        1000,
        "explore".to_string(),
    ));
    assert_eq!(state.agents[0].subagents[0].status, PrimaryStatus::Done);
    assert!(state.agents[0].subagents[0].ended_at.is_some());
}

#[test]
fn test_subagents_updated_replaces_and_clears_stale_subagent_activity() {
    let (_tmp, mut state) = make_state();
    let id = AgentId::session(AgentTool::Codex, "session-a");

    state.handle_event(AgentEvent::SessionInfo {
        id: id.clone(),
        pid: Some(44),
        session_id: "session-a".to_string(),
        branch: None,
        is_worktree: false,
        started_at: None,
    });
    state.handle_event(AgentEvent::SubagentSpawned {
        id: id.clone(),
        pid: Some(44),
        subagent: Subagent {
            name: "Nash".to_string(),
            status: PrimaryStatus::Running,
            reason: None,
            description: Some("child-a".to_string()),
            started_at: chrono::Local::now(),
            ended_at: None,
        },
    });

    assert_eq!(state.agents[0].subagents.len(), 1);
    assert!(!state.agents[0].recent_events.is_empty());

    state.handle_event(AgentEvent::SubagentsUpdated {
        id,
        pid: Some(44),
        subagents: Vec::new(),
    });

    assert!(state.agents[0].subagents.is_empty());
    assert!(state.agents[0].recent_events.iter().all(|event| !matches!(
        event.kind,
        ActivityEventKind::SubagentSpawned | ActivityEventKind::SubagentCompleted
    )));
}

#[test]
fn test_move_selection() {
    let (_tmp, mut state) = make_state();
    for pid in [100, 200, 300] {
        state.handle_event(discovered(
            AgentTool::Claude,
            pid,
            PathBuf::from(format!("/tmp/{pid}")),
            None,
        ));
    }
    assert_eq!(state.root.overview.selected_index, Some(0));
    state.move_selection_down();
    assert_eq!(state.root.overview.selected_index, Some(1));
    state.move_selection_down();
    assert_eq!(state.root.overview.selected_index, Some(2));
    state.move_selection_down();
    assert_eq!(state.root.overview.selected_index, Some(0)); // wraps to top
    state.move_selection_up();
    assert_eq!(state.root.overview.selected_index, Some(2)); // wraps to bottom
    state.move_selection_up();
    assert_eq!(state.root.overview.selected_index, Some(1));
    state.move_selection_up();
    assert_eq!(state.root.overview.selected_index, Some(0));
}

#[test]
fn test_count_by_status() {
    let (_tmp, mut state) = make_state();
    state.handle_event(discovered(AgentTool::Claude, 1, PathBuf::from("/a"), None));
    state.handle_event(discovered(
        AgentTool::OpenCode,
        2,
        PathBuf::from("/b"),
        None,
    ));
    state.handle_event(status_changed(
        AgentTool::Claude,
        1,
        PrimaryStatus::Running,
        PrimaryStatus::Waiting,
    ));
    state.handle_event(AgentEvent::SessionInfo {
        id: AgentId::session(AgentTool::Codex, "codex-hidden"),
        pid: None,
        session_id: "codex-hidden".to_string(),
        branch: Some("main".to_string()),
        is_worktree: false,
        started_at: None,
    });
    assert_eq!(state.count_by_status(PrimaryStatus::Waiting), 1);
    assert_eq!(state.count_by_status(PrimaryStatus::Running), 1);
}

#[test]
fn test_cycle_sort_mode() {
    let (_tmp, mut state) = make_state();
    assert_eq!(state.root.overview.sort_mode, SortMode::Name);
    state.cycle_sort_mode();
    assert_eq!(state.root.overview.sort_mode, SortMode::Elapsed);
    state.cycle_sort_mode();
    assert_eq!(state.root.overview.sort_mode, SortMode::Urgency);
    state.cycle_sort_mode();
    assert_eq!(state.root.overview.sort_mode, SortMode::Name);
}

#[test]
fn test_toggle_help() {
    let (_tmp, mut state) = make_state();
    assert!(!state.show_help());
    state.toggle_help();
    assert!(state.show_help());
    state.toggle_help();
    assert!(!state.show_help());
}

#[test]
fn test_visible_agents_sort_by_urgency() {
    let (_tmp, mut state) = make_state();
    state.root.overview.sort_mode = SortMode::Urgency;
    // Add agents with different statuses
    for (pid, tool) in [
        (1, AgentTool::Claude),
        (2, AgentTool::OpenCode),
        (3, AgentTool::Claude),
    ] {
        state.handle_event(discovered(
            tool,
            pid,
            PathBuf::from(format!("/tmp/{pid}")),
            None,
        ));
    }
    // Make agent 3 Waiting, agent 1 Done
    state.handle_event(status_changed(
        AgentTool::Claude,
        3,
        PrimaryStatus::Running,
        PrimaryStatus::Waiting,
    ));
    state.handle_event(status_changed(
        AgentTool::Claude,
        1,
        PrimaryStatus::Running,
        PrimaryStatus::Done,
    ));
    // Urgency sort: Waiting < Running < Done
    let visible = state.visible_agents();
    assert_eq!(visible[0].pid.unwrap(), 3); // Waiting
    assert_eq!(visible[1].pid.unwrap(), 2); // Running
    assert_eq!(visible[2].pid.unwrap(), 1); // Done
}

#[test]
fn test_visible_agents_sort_by_urgency_uses_normalized_precedence() {
    let (_tmp, mut state) = make_state();
    state.root.overview.sort_mode = SortMode::Urgency;

    for (pid, normalized) in [
        (
            1,
            NormalizedState::new(
                NormalizedStatus::Done,
                Some(NormalizedReason::ReadyForNextTask),
            ),
        ),
        (
            2,
            NormalizedState::new(NormalizedStatus::Working, Some(NormalizedReason::ToolCall)),
        ),
        (
            3,
            NormalizedState::new(NormalizedStatus::NeedsInput, Some(NormalizedReason::Review)),
        ),
    ] {
        state.handle_event(discovered(
            AgentTool::Claude,
            pid,
            PathBuf::from(format!("/tmp/{pid}")),
            None,
        ));
        state.handle_event(normalized_state_changed(AgentTool::Claude, pid, normalized));
    }

    let visible = state.visible_agents();
    assert_eq!(visible[0].pid.unwrap(), 3);
    assert_eq!(visible[1].pid.unwrap(), 2);
    assert_eq!(visible[2].pid.unwrap(), 1);
}

#[test]
fn test_visible_agents_sort_by_name() {
    let (_tmp, mut state) = make_state();
    state.root.overview.sort_mode = SortMode::Name;
    for (pid, tool) in [
        (1, AgentTool::OpenCode),
        (2, AgentTool::Claude),
        (3, AgentTool::Codex),
    ] {
        state.handle_event(discovered(
            tool,
            pid,
            PathBuf::from(format!("/tmp/{pid}")),
            None,
        ));
    }
    let visible = state.visible_agents();
    // Alphabetical: claude < codex < opencode
    assert_eq!(visible[0].id.tool(), AgentTool::Claude);
    assert_eq!(visible[1].id.tool(), AgentTool::Codex);
    assert_eq!(visible[2].id.tool(), AgentTool::OpenCode);
}

#[test]
fn test_visible_agents_filter() {
    let (_tmp, mut state) = make_state();
    for (pid, tool) in [
        (1, AgentTool::Claude),
        (2, AgentTool::OpenCode),
        (3, AgentTool::Codex),
    ] {
        state.handle_event(discovered(
            tool,
            pid,
            PathBuf::from(format!("/tmp/{pid}")),
            None,
        ));
    }
    // Filter for "claude"
    state.root.overview.filter = Some("claude".to_string());
    let visible = state.visible_agents();
    assert_eq!(visible.len(), 1);
    assert_eq!(visible[0].id.tool(), AgentTool::Claude);

    // Filter for status "RUNNING" — should match all
    state.root.overview.filter = Some("running".to_string());
    let visible = state.visible_agents();
    assert_eq!(visible.len(), 3);

    // Filter for something that matches nothing
    state.root.overview.filter = Some("zzz".to_string());
    let visible = state.visible_agents();
    assert_eq!(visible.len(), 0);
}

#[test]
fn test_filter_by_tool_name() {
    let (_tmp, mut state) = make_state();
    state.handle_event(discovered(AgentTool::Claude, 1, PathBuf::from("/a"), None));
    state.handle_event(discovered(
        AgentTool::OpenCode,
        2,
        PathBuf::from("/b"),
        None,
    ));
    state.handle_event(discovered(AgentTool::Codex, 3, PathBuf::from("/c"), None));

    state.root.overview.filter = Some("claude".to_string());
    let visible = state.visible_agents();
    assert_eq!(visible.len(), 1);
    assert_eq!(visible[0].id.tool(), AgentTool::Claude);
}

#[test]
fn test_filter_by_branch() {
    let (_tmp, mut state) = make_state();
    state.handle_event(discovered(AgentTool::Claude, 1, PathBuf::from("/a"), None));
    state.handle_event(session_info(
        AgentTool::Claude,
        1,
        "s1".to_string(),
        Some("feat/auth".to_string()),
        false,
        None,
    ));
    state.handle_event(discovered(
        AgentTool::OpenCode,
        2,
        PathBuf::from("/b"),
        None,
    ));
    state.handle_event(session_info(
        AgentTool::OpenCode,
        2,
        "s2".to_string(),
        Some("main".to_string()),
        false,
        None,
    ));

    state.root.overview.filter = Some("auth".to_string());
    let visible = state.visible_agents();
    assert_eq!(visible.len(), 1);
    assert_eq!(visible[0].pid.unwrap(), 1);
}

#[test]
fn test_filter_by_status() {
    let (_tmp, mut state) = make_state();
    state.handle_event(discovered(AgentTool::Claude, 1, PathBuf::from("/a"), None));
    state.handle_event(status_changed(
        AgentTool::Claude,
        1,
        PrimaryStatus::Running,
        PrimaryStatus::Waiting,
    ));
    state.handle_event(discovered(
        AgentTool::OpenCode,
        2,
        PathBuf::from("/b"),
        None,
    ));

    state.root.overview.filter = Some("waiting".to_string());
    let visible = state.visible_agents();
    assert_eq!(visible.len(), 1);
    assert_eq!(visible[0].pid.unwrap(), 1);
}

#[test]
fn test_filter_matches_normalized_status_and_reason() {
    let (_tmp, mut state) = make_state();
    state.handle_event(discovered(AgentTool::Claude, 1, PathBuf::from("/a"), None));
    state.handle_event(normalized_state_changed(
        AgentTool::Claude,
        1,
        NormalizedState::new(NormalizedStatus::NeedsInput, Some(NormalizedReason::Review)),
    ));

    state.root.overview.filter = Some("needs input".to_string());
    let visible = state.visible_agents();
    assert_eq!(visible.len(), 1);
    assert_eq!(visible[0].pid.unwrap(), 1);

    state.root.overview.filter = Some("review".to_string());
    let visible = state.visible_agents();
    assert_eq!(visible.len(), 1);
    assert_eq!(visible[0].pid.unwrap(), 1);
}

#[test]
fn test_filter_clamps_selected_index() {
    let (_tmp, mut state) = make_state();
    // Add 4 agents
    for pid in [1, 2, 3, 4] {
        state.handle_event(discovered(
            AgentTool::Claude,
            pid,
            PathBuf::from(format!("/tmp/{pid}")),
            None,
        ));
    }
    // Select the last agent (index 3)
    state.root.overview.selected_index = Some(3);

    // Apply a filter that matches only 1 agent by cwd
    state.start_filter();
    state.filter_push_char('1');

    // Only 1 visible agent, so selected must be clamped to 0
    assert_eq!(state.visible_agents().len(), 1);
    assert_eq!(state.root.overview.selected_index, Some(0));
}

#[test]
fn test_discover_with_real_start_time() {
    let (_tmp, mut state) = make_state();
    let past = chrono::Local::now() - chrono::Duration::hours(2);
    state.handle_event(discovered(
        AgentTool::OpenCode,
        42,
        PathBuf::from("/tmp/test"),
        Some(past),
    ));
    // The agent's started_at should be the past time, not now
    assert_eq!(state.agents[0].started_at, past);
}

#[test]
fn test_discover_without_start_time_uses_now() {
    let before = chrono::Local::now();
    let (_tmp, mut state) = make_state();
    state.handle_event(discovered(
        AgentTool::Claude,
        43,
        PathBuf::from("/tmp/test2"),
        None,
    ));
    let after = chrono::Local::now();
    assert!(state.agents[0].started_at >= before);
    assert!(state.agents[0].started_at <= after);
}

#[test]
fn test_session_info_before_discovered_is_preserved() {
    let (_tmp, mut state) = make_state();

    state.handle_event(AgentEvent::SessionInfo {
        id: AgentId::new(AgentTool::Claude, 42),
        pid: Some(42),
        session_id: "session-a".to_string(),
        branch: Some("feature/early".to_string()),
        is_worktree: true,
        started_at: None,
    });

    assert_eq!(state.agents.len(), 1);
    assert_eq!(state.agents[0].session_id, Some("session-a".to_string()));
    assert_eq!(state.agents[0].branch, Some("feature/early".to_string()));

    state.handle_event(discovered(
        AgentTool::Claude,
        42,
        PathBuf::from("/tmp/project"),
        None,
    ));

    assert_eq!(state.agents.len(), 1);
    assert_eq!(state.agents[0].cwd, PathBuf::from("/tmp/project"));
    assert_eq!(state.agents[0].session_id, Some("session-a".to_string()));
    assert_eq!(state.agents[0].branch, Some("feature/early".to_string()));
}

#[test]
fn test_codex_session_info_before_discovered_starts_unknown_not_working() {
    let (_tmp, mut state) = make_state();

    state.handle_event(AgentEvent::SessionInfo {
        id: AgentId::session(AgentTool::Codex, "session-a"),
        pid: Some(42),
        session_id: "session-a".to_string(),
        branch: Some("feature/early".to_string()),
        is_worktree: true,
        started_at: None,
    });

    assert_eq!(state.agents.len(), 1);
    assert_eq!(state.agents[0].status, PrimaryStatus::Running);
    assert_eq!(state.agents[0].normalized.status, NormalizedStatus::Unknown);
    assert_eq!(state.agents[0].normalized.reason, None);
}

#[test]
fn test_normalized_state_changed_before_discovered_is_preserved() {
    let (_tmp, mut state) = make_state();

    state.handle_event(normalized_state_changed(
        AgentTool::Claude,
        77,
        NormalizedState::new(NormalizedStatus::Blocked, Some(NormalizedReason::Approval)),
    ));

    assert_eq!(state.agents.len(), 1);
    assert_eq!(state.agents[0].normalized.status, NormalizedStatus::Blocked);
    assert_eq!(
        state.agents[0].normalized.reason,
        Some(NormalizedReason::Approval)
    );

    state.handle_event(discovered(
        AgentTool::Claude,
        77,
        PathBuf::from("/tmp/blocked"),
        None,
    ));

    assert_eq!(state.agents.len(), 1);
    assert_eq!(state.agents[0].cwd, PathBuf::from("/tmp/blocked"));
    assert_eq!(state.agents[0].normalized.status, NormalizedStatus::Blocked);
    assert_eq!(
        state.agents[0].normalized.reason,
        Some(NormalizedReason::Approval)
    );
}

#[test]
fn test_session_info_before_discovered_selects_first_visible_agent() {
    let (_tmp, mut state) = make_state();
    assert_eq!(state.root.overview.selected_index, None);

    state.handle_event(AgentEvent::SessionInfo {
        id: AgentId::new(AgentTool::Claude, 42),
        pid: Some(42),
        session_id: "session-a".to_string(),
        branch: Some("feature/early".to_string()),
        is_worktree: false,
        started_at: None,
    });

    assert_eq!(state.visible_agents().len(), 1);
    assert_eq!(state.root.overview.selected_index, Some(0));
}

#[test]
fn test_remove_exited_agents() {
    let (_tmp, mut state) = make_state();
    state.handle_event(discovered(
        AgentTool::Claude,
        100,
        PathBuf::from("/tmp/a"),
        None,
    ));
    state.handle_event(discovered(
        AgentTool::OpenCode,
        200,
        PathBuf::from("/tmp/b"),
        None,
    ));
    state.handle_event(discovered(
        AgentTool::Codex,
        300,
        PathBuf::from("/tmp/c"),
        None,
    ));
    // Exit agent 100 and 300
    state.handle_event(process_exited(AgentTool::Claude, 100));
    state.handle_event(process_exited(AgentTool::Codex, 300));
    state.root.overview.selected_index = Some(0);
    state.remove_exited();
    assert_eq!(state.agents.len(), 1);
    assert_eq!(state.agents[0].pid.unwrap(), 200);
    assert_eq!(state.root.overview.selected_index, Some(0));
}

#[test]
fn test_remove_exited_no_exited_agents() {
    let (_tmp, mut state) = make_state();
    state.handle_event(discovered(
        AgentTool::Claude,
        100,
        PathBuf::from("/tmp/a"),
        None,
    ));
    state.root.overview.selected_index = Some(0);
    state.remove_exited();
    assert_eq!(state.agents.len(), 1); // no change
    assert_eq!(state.root.overview.selected_index, Some(0));
}

#[test]
fn test_remove_exited_all_exited() {
    let (_tmp, mut state) = make_state();
    state.handle_event(discovered(
        AgentTool::Claude,
        100,
        PathBuf::from("/tmp/a"),
        None,
    ));
    state.handle_event(process_exited(AgentTool::Claude, 100));
    state.root.overview.selected_index = Some(0);
    state.remove_exited();
    assert_eq!(state.agents.len(), 0);
    assert_eq!(state.root.overview.selected_index, None);
}

#[test]
fn test_remove_selected_agent() {
    let (_tmp, mut state) = make_state();
    state.handle_event(discovered(
        AgentTool::Claude,
        100,
        PathBuf::from("/tmp/a"),
        None,
    ));
    state.handle_event(discovered(
        AgentTool::OpenCode,
        200,
        PathBuf::from("/tmp/b"),
        None,
    ));
    assert_eq!(state.agents.len(), 2);
    state.root.overview.selected_index = Some(0);
    state.remove_selected();
    assert_eq!(state.agents.len(), 1);
    assert_eq!(state.agents[0].pid.unwrap(), 200);
    assert_eq!(state.root.overview.selected_index, Some(0));
}

#[test]
fn test_remove_selected_last_item_clamps() {
    let (_tmp, mut state) = make_state();
    state.handle_event(discovered(
        AgentTool::Claude,
        100,
        PathBuf::from("/tmp/a"),
        None,
    ));
    state.handle_event(discovered(
        AgentTool::OpenCode,
        200,
        PathBuf::from("/tmp/b"),
        None,
    ));
    state.root.overview.selected_index = Some(1);
    state.remove_selected();
    assert_eq!(state.agents.len(), 1);
    assert_eq!(state.root.overview.selected_index, Some(0));
}

#[test]
fn test_remove_selected_only_item() {
    let (_tmp, mut state) = make_state();
    state.handle_event(discovered(
        AgentTool::Claude,
        100,
        PathBuf::from("/tmp/a"),
        None,
    ));
    state.root.overview.selected_index = Some(0);
    state.remove_selected();
    assert_eq!(state.agents.len(), 0);
    assert_eq!(state.root.overview.selected_index, None);
}

#[test]
fn test_remove_selected_none_selected() {
    let (_tmp, mut state) = make_state();
    state.handle_event(discovered(
        AgentTool::Claude,
        100,
        PathBuf::from("/tmp/a"),
        None,
    ));
    state.root.overview.selected_index = None;
    state.remove_selected();
    assert_eq!(state.agents.len(), 1); // no change
}

#[test]
fn test_remove_selected_with_filter_removes_correct_agent() {
    let (_tmp, mut state) = make_state();
    // Add claude (pid 100) and opencode (pid 200)
    state.handle_event(discovered(
        AgentTool::Claude,
        100,
        PathBuf::from("/tmp/a"),
        None,
    ));
    state.handle_event(discovered(
        AgentTool::OpenCode,
        200,
        PathBuf::from("/tmp/b"),
        None,
    ));
    // Filter to show only opencode
    state.root.overview.filter = Some("opencode".to_string());
    state.root.overview.selected_index = Some(0); // first visible = opencode
    state.remove_selected();
    // opencode should be removed, claude should remain
    assert_eq!(state.agents.len(), 1);
    assert_eq!(state.agents[0].id.tool(), AgentTool::Claude);
}

#[test]
fn test_rename_with_filter_renames_correct_agent() {
    let (_tmp, mut state) = make_state();
    // Add claude (pid 100) and opencode (pid 200)
    state.handle_event(discovered(
        AgentTool::Claude,
        100,
        PathBuf::from("/tmp/a"),
        None,
    ));
    state.handle_event(discovered(
        AgentTool::OpenCode,
        200,
        PathBuf::from("/tmp/b"),
        None,
    ));
    // Filter to show only opencode
    state.root.overview.filter = Some("opencode".to_string());
    state.root.overview.selected_index = Some(0); // first visible = opencode
    state.start_rename();
    state.run_overview_reducer_action(OverviewAction::UpdateRenameInput("my-oc".to_string()));
    state.confirm_rename();
    // opencode should be renamed, claude untouched
    let oc = state.agents.iter().find(|a| a.pid == Some(200)).unwrap();
    assert_eq!(oc.alias, Some("my-oc".to_string()));
    let cl = state.agents.iter().find(|a| a.pid == Some(100)).unwrap();
    assert_eq!(cl.alias, None);
}

#[test]
fn test_session_info_only_applies_earlier_start_time() {
    let (_tmp, mut state) = make_state();
    let now = chrono::Local::now();
    state.handle_event(discovered(
        AgentTool::OpenCode,
        44,
        PathBuf::from("/tmp/test3"),
        None,
    ));

    // SessionInfo with earlier time should update
    let earlier = now - chrono::Duration::hours(1);
    state.handle_event(session_info(
        AgentTool::OpenCode,
        44,
        "s1".to_string(),
        None,
        false,
        Some(earlier),
    ));
    assert_eq!(state.agents[0].started_at, earlier);

    // SessionInfo with later time should NOT update
    let later = now + chrono::Duration::hours(1);
    state.handle_event(session_info(
        AgentTool::OpenCode,
        44,
        "s1".to_string(),
        None,
        false,
        Some(later),
    ));
    // Should still be the earlier time
    assert_eq!(state.agents[0].started_at, earlier);
}

#[test]
fn test_session_info_none_started_at_does_not_overwrite_existing() {
    let (_tmp, mut state) = make_state();
    let past = chrono::Local::now() - chrono::Duration::hours(1);

    // Discover agent with a known process start time
    state.handle_event(discovered(
        AgentTool::OpenCode,
        55,
        PathBuf::from("/tmp/oc-test"),
        Some(past),
    ));
    assert_eq!(state.agents[0].started_at, past);

    // SessionInfo with started_at: None must not overwrite the process start time
    state.handle_event(AgentEvent::SessionInfo {
        id: AgentId::new(AgentTool::OpenCode, 55),
        pid: Some(55),
        session_id: "sess-55".to_string(),
        branch: None,
        is_worktree: false,
        started_at: None,
    });

    assert_eq!(
        state.agents[0].started_at, past,
        "SessionInfo with started_at: None must not overwrite the process start time"
    );
}

#[test]
fn test_codex_session_info_uses_session_identity() {
    let (_tmp, mut state) = make_state();
    state.handle_event(discovered(
        AgentTool::Codex,
        44,
        PathBuf::from("/tmp/test3"),
        None,
    ));

    state.handle_event(session_info(
        AgentTool::Codex,
        44,
        "session-a".to_string(),
        None,
        false,
        None,
    ));
    state.handle_event(session_info(
        AgentTool::Codex,
        44,
        "session-b".to_string(),
        None,
        false,
        None,
    ));

    assert_eq!(state.agents.len(), 2);
    assert_ne!(state.agents[0].session_id, state.agents[1].session_id);
}

#[test]
fn test_codex_discovery_merges_into_existing_session_row() {
    let (_tmp, mut state) = make_state();

    state.handle_event(session_info(
        AgentTool::Codex,
        44,
        "session-a".to_string(),
        None,
        false,
        None,
    ));
    state.handle_event(discovered(
        AgentTool::Codex,
        44,
        PathBuf::from("/tmp/merged"),
        None,
    ));

    assert_eq!(state.agents.len(), 1);
    assert_eq!(state.agents[0].cwd, PathBuf::from("/tmp/merged"));
    assert_eq!(state.agents[0].session_id, Some("session-a".to_string()));
    assert_eq!(state.agents[0].pid, Some(44));
}

#[test]
fn test_codex_discovery_does_not_guess_among_ambiguous_session_matches() {
    let (_tmp, mut state) = make_state();

    state.handle_event(session_info(
        AgentTool::Codex,
        44,
        "session-a".to_string(),
        None,
        false,
        None,
    ));
    state.handle_event(session_info(
        AgentTool::Codex,
        44,
        "session-b".to_string(),
        None,
        false,
        None,
    ));
    state.handle_event(discovered(
        AgentTool::Codex,
        44,
        PathBuf::from("/tmp/ambiguous"),
        None,
    ));

    assert_eq!(state.agents.len(), 2);
    let session_agents: Vec<_> = state.agents.iter().filter(|a| a.id.is_session()).collect();
    assert_eq!(session_agents.len(), 2);
    assert!(session_agents
        .iter()
        .all(|a| a.cwd == std::path::Path::new("/tmp/ambiguous")));
    assert!(state
        .agents
        .iter()
        .all(|a| a.id.is_session() && a.pid == Some(44)));
}

#[test]
fn test_codex_process_exit_retires_all_session_rows_sharing_pid() {
    let (_tmp, mut state) = make_state();

    state.handle_event(session_info(
        AgentTool::Codex,
        44,
        "session-a".to_string(),
        None,
        false,
        None,
    ));
    state.handle_event(session_info(
        AgentTool::Codex,
        44,
        "session-b".to_string(),
        None,
        false,
        None,
    ));

    state.handle_event(process_exited(AgentTool::Codex, 44));

    assert_eq!(state.agents.len(), 2);
    assert!(state
        .agents
        .iter()
        .all(|a| a.status == PrimaryStatus::Exited && a.pid == Some(44)));
}

#[test]
fn test_codex_process_only_row_is_removed_on_exit() {
    let (_tmp, mut state) = make_state();

    state.handle_event(discovered(
        AgentTool::Codex,
        45337,
        PathBuf::from("/tmp/nexus"),
        None,
    ));
    assert_eq!(state.agents.len(), 1);
    assert!(state.agents[0].id.is_process());

    state.handle_event(process_exited(AgentTool::Codex, 45337));

    assert!(state.agents.is_empty());
    assert!(state.visible_agents().is_empty());
}

#[test]
fn test_codex_session_info_absorbs_stale_process_row_after_ambiguous_pid_resolves() {
    let (_tmp, mut state) = make_state();

    state.handle_event(AgentEvent::Discovered {
        id: AgentId::session(AgentTool::Codex, "session-a"),
        pid: None,
        cwd: PathBuf::from("/tmp/ambiguous"),
        started_at: None,
    });
    state.handle_event(discovered(
        AgentTool::Codex,
        77,
        PathBuf::from("/tmp/ambiguous"),
        None,
    ));

    assert_eq!(state.agents.len(), 2);
    assert_eq!(state.agents.iter().filter(|a| a.id.is_session()).count(), 1);
    assert_eq!(state.agents.iter().filter(|a| a.id.is_process()).count(), 1);

    state.handle_event(session_info(
        AgentTool::Codex,
        77,
        "session-a".to_string(),
        None,
        false,
        None,
    ));

    assert_eq!(state.agents.len(), 1);
    assert!(state.agents[0].id.is_session());
    assert_eq!(state.agents[0].pid, Some(77));
    assert_eq!(state.agents[0].cwd, PathBuf::from("/tmp/ambiguous"));
    assert_eq!(state.agents[0].session_id, Some("session-a".to_string()));
}

#[test]
fn test_codex_session_info_preserves_terminal_status() {
    let (_tmp, mut state) = make_state();

    state.handle_event(session_info(
        AgentTool::Codex,
        44,
        "session-a".to_string(),
        None,
        false,
        None,
    ));
    state.handle_event(AgentEvent::StatusChanged {
        id: AgentId::session(AgentTool::Codex, "session-a"),
        pid: Some(44),
        old: PrimaryStatus::Running,
        new: PrimaryStatus::Done,
    });

    assert_eq!(state.agents[0].status, PrimaryStatus::Done);

    state.handle_event(session_info(
        AgentTool::Codex,
        55,
        "session-a".to_string(),
        None,
        false,
        None,
    ));

    assert_eq!(state.agents.len(), 1);
    assert!(state.agents[0].id.is_session());
    assert_eq!(state.agents[0].status, PrimaryStatus::Done);
    assert_eq!(state.agents[0].pid, Some(55));
}

#[test]
fn test_codex_process_discovery_does_not_revive_done_session() {
    let (_tmp, mut state) = make_state();

    state.handle_event(session_info(
        AgentTool::Codex,
        15642,
        "session-a".to_string(),
        None,
        false,
        None,
    ));
    state.handle_event(AgentEvent::StatusChanged {
        id: AgentId::session(AgentTool::Codex, "session-a"),
        pid: Some(15642),
        old: PrimaryStatus::Running,
        new: PrimaryStatus::Done,
    });

    state.handle_event(discovered(
        AgentTool::Codex,
        15642,
        PathBuf::from("/tmp/project"),
        None,
    ));

    assert_eq!(state.agents.len(), 1);
    assert!(state.agents[0].id.is_session());
    assert_eq!(state.agents[0].status, PrimaryStatus::Done);
    assert_eq!(state.agents[0].pid, Some(15642));
}

#[test]
fn test_codex_session_info_with_none_pid_preserves_existing_pid() {
    // A SessionInfo with pid=None must NOT clear a pid that was already bound.
    // The pid should only be cleared by ProcessExited, not by a stale snapshot
    // that omits the pid because the thread is outside the conservative binding
    // window.
    let (_tmp, mut state) = make_state();

    state.handle_event(session_info(
        AgentTool::Codex,
        44,
        "session-a".to_string(),
        None,
        false,
        None,
    ));
    assert_eq!(state.agents[0].pid, Some(44));

    state.handle_event(AgentEvent::SessionInfo {
        id: AgentId::session(AgentTool::Codex, "session-a"),
        pid: None,
        session_id: "session-a".to_string(),
        branch: None,
        is_worktree: false,
        started_at: None,
    });

    assert_eq!(state.agents.len(), 1);
    assert!(state.agents[0].id.is_session());
    // pid must be preserved — not cleared — when SessionInfo arrives with pid=None
    assert_eq!(state.agents[0].pid, Some(44));
}

#[test]
fn test_codex_session_stays_visible_when_session_info_pid_is_none() {
    // When a Codex session already has a pid, a subsequent SessionInfo with
    // pid=None should preserve the pid so the session remains visible.
    let (_tmp, mut state) = make_state();
    state.handle_event(discovered(
        AgentTool::Claude,
        10,
        PathBuf::from("/tmp/claude"),
        None,
    ));
    state.handle_event(AgentEvent::SessionInfo {
        id: AgentId::session(AgentTool::Codex, "codex-a"),
        pid: Some(20),
        session_id: "codex-a".to_string(),
        branch: Some("main".to_string()),
        is_worktree: false,
        started_at: None,
    });
    state.root.overview.selected_index = Some(1);

    state.handle_event(AgentEvent::SessionInfo {
        id: AgentId::session(AgentTool::Codex, "codex-a"),
        pid: None,
        session_id: "codex-a".to_string(),
        branch: Some("main".to_string()),
        is_worktree: false,
        started_at: None,
    });

    // Both agents should still be visible (pid preserved on Codex session)
    assert_eq!(state.visible_agents().len(), 2);
    assert_eq!(state.root.overview.selected_index, Some(1));
}

#[test]
fn test_codex_session_without_pid_is_not_visible() {
    let (_tmp, mut state) = make_state();
    state.handle_event(AgentEvent::SessionInfo {
        id: AgentId::session(AgentTool::Codex, "codex-a"),
        pid: None,
        session_id: "codex-a".to_string(),
        branch: Some("main".to_string()),
        is_worktree: false,
        started_at: None,
    });
    assert!(state.visible_agents().is_empty());
}

#[test]
fn test_codex_session_with_pid_is_visible() {
    let (_tmp, mut state) = make_state();
    state.handle_event(AgentEvent::SessionInfo {
        id: AgentId::session(AgentTool::Codex, "codex-a"),
        pid: Some(42),
        session_id: "codex-a".to_string(),
        branch: Some("main".to_string()),
        is_worktree: false,
        started_at: None,
    });
    assert_eq!(state.visible_agents().len(), 1);
}

#[test]
fn test_normalized_state_changed_updates_agent_normalized_state() {
    let (_tmp, mut state) = make_state();
    let id = AgentId::new(AgentTool::Claude, 42);

    state.handle_event(discovered(
        AgentTool::Claude,
        42,
        PathBuf::from("/tmp/normalized"),
        None,
    ));
    state.handle_event(AgentEvent::NormalizedStateChanged {
        id,
        pid: Some(42),
        state: NormalizedState::new(NormalizedStatus::NeedsInput, Some(NormalizedReason::Review)),
    });

    assert_eq!(
        state.agents[0].normalized.status,
        NormalizedStatus::NeedsInput
    );
    assert_eq!(
        state.agents[0].normalized.reason,
        Some(NormalizedReason::Review)
    );
}

#[test]
fn test_codex_session_discovery_revives_exited_session_row() {
    let (_tmp, mut state) = make_state();

    state.handle_event(session_info(
        AgentTool::Codex,
        44,
        "session-a".to_string(),
        None,
        false,
        None,
    ));
    state.handle_event(process_exited(AgentTool::Codex, 44));

    assert_eq!(state.agents[0].status, PrimaryStatus::Exited);

    state.handle_event(AgentEvent::Discovered {
        id: AgentId::session(AgentTool::Codex, "session-a"),
        pid: Some(44),
        cwd: PathBuf::from("/tmp/revive"),
        started_at: None,
    });

    assert_eq!(state.agents.len(), 1);
    assert!(state.agents[0].id.is_session());
    assert_eq!(state.agents[0].status, PrimaryStatus::Running);
    assert_eq!(state.agents[0].pid, Some(44));
    assert_eq!(state.agents[0].cwd, PathBuf::from("/tmp/revive"));
}

#[test]
fn test_codex_discovery_does_not_revive_exited_session_row_on_pid_reuse() {
    let (_tmp, mut state) = make_state();

    state.handle_event(session_info(
        AgentTool::Codex,
        44,
        "session-a".to_string(),
        None,
        false,
        None,
    ));
    state.handle_event(process_exited(AgentTool::Codex, 44));

    state.handle_event(discovered(
        AgentTool::Codex,
        44,
        PathBuf::from("/tmp/reused"),
        None,
    ));

    assert_eq!(state.agents.len(), 2);
    assert_eq!(state.agents.iter().filter(|a| a.id.is_session()).count(), 1);
    assert_eq!(state.agents.iter().filter(|a| a.id.is_process()).count(), 1);

    let exited_session = state.agents.iter().find(|a| a.id.is_session()).unwrap();
    assert_eq!(exited_session.status, PrimaryStatus::Exited);
    assert_eq!(exited_session.pid, Some(44));

    let process_agent = state.agents.iter().find(|a| a.id.is_process()).unwrap();
    assert_eq!(process_agent.status, PrimaryStatus::Running);
    assert_eq!(process_agent.cwd, PathBuf::from("/tmp/reused"));
}

#[test]
fn test_codex_process_discovery_revives_exited_session_row_with_same_pid() {
    let (_tmp, mut state) = make_state();

    state.handle_event(AgentEvent::Discovered {
        id: AgentId::session(AgentTool::Codex, "session-a"),
        pid: Some(44),
        cwd: PathBuf::from("/tmp/revive"),
        started_at: None,
    });
    state.handle_event(session_info(
        AgentTool::Codex,
        44,
        "session-a".to_string(),
        None,
        false,
        None,
    ));
    state.handle_event(process_exited(AgentTool::Codex, 44));

    assert_eq!(state.agents.len(), 1);
    assert!(state.agents[0].id.is_session());
    assert_eq!(state.agents[0].status, PrimaryStatus::Exited);

    state.handle_event(discovered(
        AgentTool::Codex,
        44,
        PathBuf::from("/tmp/revive"),
        None,
    ));

    assert_eq!(state.agents.len(), 1);
    assert!(state.agents[0].id.is_session());
    assert_eq!(state.agents[0].pid, Some(44));
    assert_eq!(state.agents[0].status, PrimaryStatus::Running);
    assert_eq!(state.agents[0].cwd, PathBuf::from("/tmp/revive"));
}

#[test]
fn test_codex_subagent_completion_matches_stable_key_and_keeps_human_name() {
    let (_tmp, mut state) = make_state();
    let id = AgentId::session(AgentTool::Codex, "session-a");

    state.handle_event(AgentEvent::SessionInfo {
        id: id.clone(),
        pid: Some(44),
        session_id: "session-a".to_string(),
        branch: None,
        is_worktree: false,
        started_at: None,
    });
    state.handle_event(AgentEvent::SubagentSpawned {
        id: id.clone(),
        pid: Some(44),
        subagent: Subagent {
            name: "Nash".to_string(),
            status: PrimaryStatus::Running,
            reason: None,
            description: Some("child-a".to_string()),
            started_at: chrono::Local::now(),
            ended_at: None,
        },
    });

    state.handle_event(AgentEvent::SubagentCompleted {
        id,
        pid: Some(44),
        subagent_name: "child-a".to_string(),
    });

    assert_eq!(state.agents[0].subagents.len(), 1);
    assert_eq!(state.agents[0].subagents[0].name, "Nash");
    assert_eq!(state.agents[0].subagents[0].status, PrimaryStatus::Done);
}

#[test]
fn test_agent_identity_session_has_no_pid() {
    let id = AgentId::session(AgentTool::Codex, "session-a");
    assert_eq!(
        id.identity,
        AgentIdentity::Session {
            tool: AgentTool::Codex,
            session_id: "session-a".to_string(),
        }
    );
    assert_eq!(id.pid(), None);
}

#[test]
fn test_claude_session_info_stays_process_keyed() {
    let (_tmp, mut state) = make_state();
    state.handle_event(discovered(
        AgentTool::Claude,
        44,
        PathBuf::from("/tmp/test3"),
        None,
    ));

    state.handle_event(session_info(
        AgentTool::Claude,
        44,
        "claude-session-a".to_string(),
        None,
        false,
        None,
    ));
    state.handle_event(session_info(
        AgentTool::Claude,
        44,
        "claude-session-b".to_string(),
        None,
        false,
        None,
    ));

    assert_eq!(state.agents.len(), 1);
    assert_eq!(state.agents[0].pid.unwrap(), 44);
    assert_eq!(
        state.agents[0].session_id,
        Some("claude-session-b".to_string())
    );
}

#[test]
fn test_mixed_process_and_session_identities_stay_independent() {
    let (_tmp, mut state) = make_state();

    state.handle_event(discovered(
        AgentTool::Claude,
        44,
        PathBuf::from("/tmp/claude"),
        None,
    ));
    state.handle_event(session_info(
        AgentTool::Codex,
        55,
        "codex-session-a".to_string(),
        None,
        false,
        None,
    ));

    assert_eq!(state.agents.len(), 2);
    assert_eq!(state.visible_agents().len(), 2);

    state.root.overview.selected_index = Some(1);
    state.start_rename();
    assert_eq!(
        state.root.overview.mode,
        OverviewMode::Renaming {
            agent_id: state.agents[1].id.clone(),
            input: String::new()
        }
    );
    state.run_overview_reducer_action(OverviewAction::UpdateRenameInput(
        "codex-session".to_string(),
    ));
    state.confirm_rename();

    let claude = state
        .agents
        .iter()
        .find(|agent| agent.id.tool() == AgentTool::Claude)
        .unwrap();
    let codex = state
        .agents
        .iter()
        .find(|agent| agent.id.is_session())
        .unwrap();
    assert_eq!(claude.alias, None);
    assert_eq!(codex.alias.as_deref(), Some("codex-session"));
    assert_eq!(codex.display_name(), "codex-session");

    state.handle_event(status_changed(
        AgentTool::Claude,
        44,
        PrimaryStatus::Running,
        PrimaryStatus::Done,
    ));

    let claude = state
        .agents
        .iter()
        .find(|agent| agent.id.tool() == AgentTool::Claude)
        .unwrap();
    let codex = state
        .agents
        .iter()
        .find(|agent| agent.id.is_session())
        .unwrap();
    assert_eq!(claude.status, PrimaryStatus::Done);
    assert_eq!(codex.status, PrimaryStatus::Running);
}

#[test]
fn test_default_sort_mode_is_name() {
    let (_tmp, state) = make_state();
    assert_eq!(state.root.overview.sort_mode, SortMode::Name);
}

#[test]
fn test_session_info_none_start_time_keeps_existing() {
    let (_tmp, mut state) = make_state();
    let past = chrono::Local::now() - chrono::Duration::hours(3);
    state.handle_event(discovered(
        AgentTool::Codex,
        45,
        PathBuf::from("/tmp/test4"),
        Some(past),
    ));
    state.handle_event(session_info(
        AgentTool::Codex,
        45,
        "s2".to_string(),
        None,
        false,
        None,
    ));
    assert_eq!(state.agents[0].started_at, past);
}

#[test]
fn test_start_rename_sets_mode() {
    let (_tmp, mut state) = make_state();
    state.handle_event(discovered(
        AgentTool::Claude,
        100,
        PathBuf::from("/tmp/a"),
        None,
    ));
    state.root.overview.selected_index = Some(0);
    state.start_rename();
    assert!(matches!(
        state.root.overview.mode,
        OverviewMode::Renaming { ref input, .. } if input.is_empty()
    ));
}

#[test]
fn test_confirm_rename_sets_alias() {
    let (_tmp, mut state) = make_state();
    state.handle_event(discovered(
        AgentTool::Claude,
        100,
        PathBuf::from("/tmp/a"),
        None,
    ));
    state.root.overview.selected_index = Some(0);
    state.start_rename();
    state.run_overview_reducer_action(OverviewAction::UpdateRenameInput("my-claude".to_string()));
    state.confirm_rename();
    assert_eq!(state.root.overview.mode, OverviewMode::Normal);
    assert_eq!(state.agents[0].alias, Some("my-claude".to_string()));
    assert_eq!(state.agents[0].display_name(), "my-claude");
}

#[test]
fn test_confirm_rename_empty_keeps_existing_name() {
    let (_tmp, mut state) = make_state();
    state.handle_event(discovered(
        AgentTool::Claude,
        100,
        PathBuf::from("/tmp/a"),
        None,
    ));
    state.root.overview.selected_index = Some(0);
    state.agents[0].alias = Some("old-name".to_string());
    state.sync_root_overview_agents();
    state.start_rename();
    state.run_overview_reducer_action(OverviewAction::UpdateRenameInput(String::new()));
    state.confirm_rename();
    assert_eq!(state.agents[0].alias.as_deref(), Some("old-name"));
    assert_eq!(state.agents[0].display_name(), "old-name");
}

#[test]
fn test_cancel_rename_preserves_alias() {
    let (_tmp, mut state) = make_state();
    state.handle_event(discovered(
        AgentTool::Claude,
        100,
        PathBuf::from("/tmp/a"),
        None,
    ));
    state.root.overview.selected_index = Some(0);
    state.agents[0].alias = Some("keep-this".to_string());
    state.sync_root_overview_agents();
    state.start_rename();
    state.run_overview_reducer_action(OverviewAction::UpdateRenameInput("nope".to_string()));
    state.cancel_rename();
    assert_eq!(state.root.overview.mode, OverviewMode::Normal);
    assert_eq!(state.agents[0].alias, Some("keep-this".to_string()));
}

#[test]
fn test_start_rename_no_selection() {
    let (_tmp, mut state) = make_state();
    state.root.overview.selected_index = None;
    state.start_rename();
    assert_eq!(state.root.overview.mode, OverviewMode::Normal);
}

#[test]
fn test_todos_updated_sets_todos() {
    let (_tmp, mut state) = make_state();
    state.handle_event(discovered(
        AgentTool::Claude,
        100,
        PathBuf::from("/tmp"),
        None,
    ));
    let todos = vec![
        TodoItem {
            content: "Fix bug".to_string(),
            status: TodoStatus::InProgress,
            priority: TodoPriority::High,
        },
        TodoItem {
            content: "Write tests".to_string(),
            status: TodoStatus::Pending,
            priority: TodoPriority::High,
        },
    ];
    state.handle_event(todos_updated(AgentTool::Claude, 100, todos.clone()));
    assert_eq!(state.agents[0].todos, todos);
}

#[test]
fn test_todos_updated_replaces_previous() {
    let (_tmp, mut state) = make_state();
    state.handle_event(discovered(
        AgentTool::Claude,
        100,
        PathBuf::from("/tmp"),
        None,
    ));
    let todos_v1 = vec![TodoItem {
        content: "Task A".to_string(),
        status: TodoStatus::Pending,
        priority: TodoPriority::Medium,
    }];
    state.handle_event(todos_updated(AgentTool::Claude, 100, todos_v1));
    let todos_v2 = vec![
        TodoItem {
            content: "Task A".to_string(),
            status: TodoStatus::Completed,
            priority: TodoPriority::Medium,
        },
        TodoItem {
            content: "Task B".to_string(),
            status: TodoStatus::InProgress,
            priority: TodoPriority::High,
        },
    ];
    state.handle_event(todos_updated(AgentTool::Claude, 100, todos_v2.clone()));
    assert_eq!(state.agents[0].todos, todos_v2);
}

#[test]
fn test_todos_updated_ignores_unknown_pid() {
    let (_tmp, mut state) = make_state();
    state.handle_event(discovered(
        AgentTool::Claude,
        100,
        PathBuf::from("/tmp"),
        None,
    ));
    state.handle_event(todos_updated(
        AgentTool::Claude,
        999,
        vec![TodoItem {
            content: "Ghost".to_string(),
            status: TodoStatus::Pending,
            priority: TodoPriority::Low,
        }],
    ));
    assert!(state.agents[0].todos.is_empty());
}

// ── Fix #12: status count cache ───────────────────────────────────────────────

#[test]
fn test_count_by_status_zero_initially() {
    let (_tmp, state) = make_state();
    assert_eq!(state.count_by_status(PrimaryStatus::Running), 0);
    assert_eq!(state.count_by_status(PrimaryStatus::Waiting), 0);
    assert_eq!(state.count_by_status(PrimaryStatus::Exited), 0);
    assert_eq!(state.count_by_status(PrimaryStatus::Done), 0);
}

#[test]
fn test_count_by_status_increments_on_discovered() {
    let (_tmp, mut state) = make_state();
    state.handle_event(discovered(AgentTool::OpenCode, 1, "/tmp/a", None));
    assert_eq!(state.count_by_status(PrimaryStatus::Running), 1);

    state.handle_event(discovered(AgentTool::Claude, 2, "/tmp/b", None));
    assert_eq!(state.count_by_status(PrimaryStatus::Running), 2);
}

#[test]
fn test_count_by_status_updates_on_status_changed() {
    let (_tmp, mut state) = make_state();
    state.handle_event(discovered(AgentTool::OpenCode, 1, "/tmp/a", None));
    state.handle_event(discovered(AgentTool::OpenCode, 2, "/tmp/b", None));

    state.handle_event(status_changed(
        AgentTool::OpenCode,
        1,
        PrimaryStatus::Running,
        PrimaryStatus::Waiting,
    ));

    assert_eq!(state.count_by_status(PrimaryStatus::Running), 1);
    assert_eq!(state.count_by_status(PrimaryStatus::Waiting), 1);
}

#[test]
fn test_count_by_status_updates_on_process_exited() {
    let (_tmp, mut state) = make_state();
    state.handle_event(discovered(AgentTool::OpenCode, 1, "/tmp/a", None));
    state.handle_event(discovered(AgentTool::OpenCode, 2, "/tmp/b", None));
    state.handle_event(process_exited(AgentTool::OpenCode, 1));

    // OpenCode process agents are removed on exit (not just marked Exited)
    assert_eq!(state.count_by_status(PrimaryStatus::Running), 1);
    assert_eq!(state.count_by_status(PrimaryStatus::Exited), 0);
}

#[test]
fn test_count_by_status_consistent_with_agents_vec() {
    let (_tmp, mut state) = make_state();
    state.handle_event(discovered(AgentTool::OpenCode, 1, "/tmp/a", None));
    state.handle_event(discovered(AgentTool::OpenCode, 2, "/tmp/b", None));
    state.handle_event(discovered(AgentTool::Claude, 3, "/tmp/c", None));
    state.handle_event(status_changed(
        AgentTool::OpenCode,
        1,
        PrimaryStatus::Running,
        PrimaryStatus::Waiting,
    ));
    state.handle_event(process_exited(AgentTool::OpenCode, 2));

    for status in [
        PrimaryStatus::Running,
        PrimaryStatus::Waiting,
        PrimaryStatus::Exited,
        PrimaryStatus::Done,
    ] {
        let expected = state.agents.iter().filter(|a| a.status == status).count();
        assert_eq!(
            state.count_by_status(status),
            expected,
            "count_by_status({status:?}) mismatch"
        );
    }
}

#[test]
fn codex_tab_active_profile_can_be_set() {
    let mut state = AppState::new(Config::default());
    state.set_active_codex_profile(Some("profile-01".into()));
    assert_eq!(state.active_codex_profile_id(), Some("profile-01"));
}
