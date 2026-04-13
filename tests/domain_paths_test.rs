use std::fs;
use std::path::Path;

use nexus::domain::accounts::model::{
    AccountEntry, AuthMetadata, ClaudeProfile, ClaudeProfileMetadata, CodexProfile, ProfileMetadata,
};
use nexus::domain::accounts::provider::{Provider, ProviderFilter};
use nexus::domain::accounts::quota::{
    Confidence, LimitState, QuotaCache, QuotaSnapshot, QuotaWindow,
};
use nexus::domain::accounts::{AccountEntry as DomainAccountEntry, Provider as DomainProvider};
use nexus::domain::agents::activity::{
    Activity, ActivityEvent, ActivityEventKind, Subagent, TodoItem, TodoPriority, TodoStatus,
};
use nexus::domain::agents::ids::{AgentId, AgentIdentity, AgentTool};
use nexus::domain::agents::model::{Agent, SortMode};
use nexus::domain::agents::status::{
    NormalizedReason, NormalizedState, NormalizedStatus, PrimaryStatus,
};
use nexus::domain::agents::{Agent as DomainAgent, AgentTool as DomainAgentTool};
use nexus::domain::shared::time::{
    format_duration, format_duration_short, from_epoch_millis, from_epoch_secs, from_rfc3339,
};
use nexus::domain::system::health::CodexBackendHealth;
use nexus::domain::system::notification::DetectorNotification;
use nexus::domain::system::{
    CodexBackendHealth as DomainCodexBackendHealth, DetectorKind,
    DetectorNotification as DomainDetectorNotification,
};
use nexus::{AgentEvent, AppState, RootState};

#[test]
fn domain_modules_export_expected_paths() {
    let _ = std::any::TypeId::of::<AgentTool>();
    let _ = std::any::TypeId::of::<AgentIdentity>();
    let _ = std::any::TypeId::of::<AgentId>();
    let _ = std::any::TypeId::of::<PrimaryStatus>();
    let _ = std::any::TypeId::of::<NormalizedStatus>();
    let _ = std::any::TypeId::of::<NormalizedReason>();
    let _ = std::any::TypeId::of::<NormalizedState>();
    let _ = std::any::TypeId::of::<Activity>();
    let _ = std::any::TypeId::of::<ActivityEventKind>();
    let _ = std::any::TypeId::of::<ActivityEvent>();
    let _ = std::any::TypeId::of::<Subagent>();
    let _ = std::any::TypeId::of::<TodoStatus>();
    let _ = std::any::TypeId::of::<TodoPriority>();
    let _ = std::any::TypeId::of::<TodoItem>();
    let _ = std::any::TypeId::of::<Agent>();
    let _ = std::any::TypeId::of::<SortMode>();
    let _ = std::any::TypeId::of::<Provider>();
    let _ = std::any::TypeId::of::<ProviderFilter>();
    let _ = std::any::TypeId::of::<AccountEntry>();
    let _ = std::any::TypeId::of::<AuthMetadata>();
    let _ = std::any::TypeId::of::<CodexProfile>();
    let _ = std::any::TypeId::of::<ProfileMetadata>();
    let _ = std::any::TypeId::of::<ClaudeProfile>();
    let _ = std::any::TypeId::of::<ClaudeProfileMetadata>();
    let _ = std::any::TypeId::of::<QuotaWindow>();
    let _ = std::any::TypeId::of::<QuotaSnapshot>();
    let _ = std::any::TypeId::of::<QuotaCache>();
    let _ = std::any::TypeId::of::<Confidence>();
    let _ = std::any::TypeId::of::<LimitState>();
    let _ = std::any::TypeId::of::<CodexBackendHealth>();
    let _ = std::any::TypeId::of::<DetectorKind>();
    let _ = std::any::TypeId::of::<DetectorNotification>();
    let _ = std::any::TypeId::of::<DomainProvider>();
    let _ = std::any::TypeId::of::<DomainAccountEntry>();
    let _ = std::any::TypeId::of::<DomainAgentTool>();
    let _ = std::any::TypeId::of::<DomainAgent>();
    let _ = std::any::TypeId::of::<DomainCodexBackendHealth>();
    let _ = std::any::TypeId::of::<DomainDetectorNotification>();
    let _ = format_duration as fn(chrono::Duration) -> String;
    let _ = format_duration_short as fn(chrono::Duration) -> String;
    let _ = from_epoch_millis as fn(i64) -> Option<chrono::DateTime<chrono::Local>>;
    let _ = from_epoch_secs as fn(i64) -> Option<chrono::DateTime<chrono::Local>>;
    let _ = from_rfc3339 as fn(&str) -> Option<chrono::DateTime<chrono::Local>>;
}

#[test]
fn accounts_domain_owns_its_provider_and_entry_types() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let provider = fs::read_to_string(repo_root.join("src/domain/accounts/provider.rs")).unwrap();
    let model = fs::read_to_string(repo_root.join("src/domain/accounts/model.rs")).unwrap();

    assert!(
        !provider.contains("crate::app"),
        "provider.rs should define domain-owned types instead of importing them from app",
    );
    assert!(
        !model.contains("crate::app"),
        "model.rs should define domain-owned types instead of importing them from app",
    );
}

#[test]
fn selected_production_modules_no_longer_depend_on_types_shim() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let files = [
        "src/event.rs",
        "src/app/action.rs",
        "src/detect/process.rs",
        "src/detect/registry.rs",
        "src/ui/theme.rs",
    ];

    for file in files {
        let content = fs::read_to_string(repo_root.join(file)).unwrap();
        assert!(
            !content.contains("crate::types"),
            "{file} should import from domain modules directly instead of the transitional types shim",
        );
    }
}

#[test]
fn ui_root_no_longer_exports_accounts_tab_compatibility_module() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let ui_root = fs::read_to_string(repo_root.join("src/ui/mod.rs")).unwrap();

    assert!(
        !ui_root.contains("pub mod accounts_tab;"),
        "src/ui/mod.rs should not keep exporting the transitional accounts_tab module",
    );
}

#[test]
fn ui_and_overview_reducer_modules_no_longer_depend_on_types_shim() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let files = [
        "src/ui/mod.rs",
        "src/ui/activity.rs",
        "src/ui/agent_list.rs",
        "src/ui/detail.rs",
        "src/app/reducer/overview.rs",
    ];

    for file in files {
        let content = fs::read_to_string(repo_root.join(file)).unwrap();
        assert!(
            !content.contains("crate::types"),
            "{file} should import domain agent types directly instead of the transitional types shim",
        );
    }
}

#[test]
fn app_state_no_longer_exposes_legacy_overview_fields() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let app_mod = fs::read_to_string(repo_root.join("src/app/mod.rs")).unwrap();

    let dead_fields = [
        "pub selected: Option<usize>",
        "pub sort_mode: SortMode",
        "pub filter: Option<String>",
        "pub filter_mode: bool",
        "pub filter_input: String",
        "pub rename_mode: bool",
        "pub rename_input: String",
    ];

    for dead_field in dead_fields {
        assert!(
            !app_mod.contains(dead_field),
            "AppState should no longer expose legacy Overview state outside root.overview: {dead_field}",
        );
    }
}

#[test]
fn codex_and_opencode_type_modules_no_longer_depend_on_types_shim() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let files = [
        "src/detect/opencode/mod.rs",
        "src/detect/opencode/types.rs",
        "src/detect/opencode/query.rs",
        "src/detect/codex/types.rs",
        "src/detect/codex/rollout.rs",
    ];

    for file in files {
        let content = fs::read_to_string(repo_root.join(file)).unwrap();
        assert!(
            !content.contains("crate::types"),
            "{file} should import domain types directly instead of the transitional types shim",
        );
    }
}

#[test]
fn claude_and_detector_core_modules_no_longer_depend_on_types_shim() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let files = [
        "src/detect/claude/mod.rs",
        "src/detect/claude/inference.rs",
        "src/detect/claude/types.rs",
        "src/detect/mod.rs",
        "src/detect/opencode/inference.rs",
    ];

    for file in files {
        let content = fs::read_to_string(repo_root.join(file)).unwrap();
        assert!(
            !content.contains("crate::types"),
            "{file} should import domain types directly instead of the transitional types shim",
        );
    }
}

#[test]
fn app_core_modules_no_longer_depend_on_types_shim() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let files = ["src/app/state/mod.rs", "src/app/mod.rs"];

    for file in files {
        let content = fs::read_to_string(repo_root.join(file)).unwrap();
        assert!(
            !content.contains("crate::types"),
            "{file} should import domain modules directly instead of the transitional types shim",
        );
    }
}

#[test]
fn crate_root_exposes_behavioral_app_api() {
    let _ = std::any::TypeId::of::<AppState>();
    let _ = std::any::TypeId::of::<RootState>();
    let _ = std::any::TypeId::of::<AgentEvent>();

    let app = AppState::from_root(RootState::default());
    assert_eq!(app.root().overview().visible_agent_count(), 0);
    assert!(app.visible_agents().is_empty());
    assert_eq!(app.primary_detector_issue(), None);
}

#[test]
fn runtime_semantic_input_delegates_back_to_runtime_session() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let runtime_input =
        fs::read_to_string(repo_root.join("src/services/runtime/input.rs")).unwrap();

    assert!(
        runtime_input.contains("session.dispatch_action(action).await"),
        "src/services/runtime/input.rs should hand semantic app actions back to RuntimeSession instead of matching them inline",
    );
    assert!(
        !runtime_input.contains("SystemAction::ScrollHelpDown => state.scroll_help_down()"),
        "runtime input helpers should not special-case help scrolling once system actions flow through the app reducer path",
    );
}

#[test]
fn detector_events_and_actions_use_typed_detector_ids() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let event_rs = fs::read_to_string(repo_root.join("src/event.rs")).unwrap();
    let action_rs = fs::read_to_string(repo_root.join("src/app/action.rs")).unwrap();

    assert!(!event_rs.contains("detector: String"));
    assert!(!action_rs.contains("detector: String"));
    assert!(event_rs.contains("DetectorKind"));
    assert!(action_rs.contains("DetectorKind"));
}

#[test]
fn root_state_accessors_preserve_explicit_boundaries() {
    let root = RootState::default();

    assert_eq!(root.workspace().active().label(), "Overview");
    assert_eq!(root.overview().visible_agent_count(), 0);
    assert!(root.accounts().visible_entries().is_empty());
    assert!(root.system().detector_health().is_empty());
}

#[test]
fn remaining_detector_modules_no_longer_depend_on_types_shim() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let files = [
        "src/detect/claude/parser.rs",
        "src/detect/codex/snapshot.rs",
        "src/detect/codex/mod.rs",
    ];

    for file in files {
        let content = fs::read_to_string(repo_root.join(file)).unwrap();
        assert!(
            !content.contains("crate::types"),
            "{file} should import domain types directly instead of the transitional types shim",
        );
    }
}

#[test]
fn crate_root_no_longer_exports_types_shim_module() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let lib_rs = fs::read_to_string(repo_root.join("src/lib.rs")).unwrap();

    assert!(
        !lib_rs.contains("pub mod types;"),
        "src/lib.rs should stop exporting the transitional types shim once production code no longer depends on it",
    );
}

#[test]
fn lib_exports_only_selected_top_level_modules_and_runtime_entrypoints() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let lib_rs = fs::read_to_string(repo_root.join("src/lib.rs")).unwrap();

    for forbidden in [
        "pub mod app;",
        "pub mod detect;",
        "pub mod event;",
        "pub mod infra;",
        "pub mod input;",
        "pub mod services;",
        "pub mod ui;",
    ] {
        assert!(
            !lib_rs.contains(forbidden),
            "crate root should not publicly expose internal implementation module {forbidden}",
        );
    }

    for required in [
        "pub mod claude_profiles;",
        "pub mod codex_profiles;",
        "pub mod config;",
        "pub mod domain;",
        "pub mod runtime_support;",
        "pub use config::{Cli, Config};",
        "pub use infra::config::migrate_legacy_dirs;",
        "pub use runtime_support::run_blocking_compatible;",
        "pub use services::runtime::run;",
    ] {
        assert!(
            lib_rs.contains(required),
            "crate root should expose curated public entrypoint {required}",
        );
    }
}

#[test]
fn app_accounts_bridge_no_longer_exposes_dead_legacy_input_helpers() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let accounts = fs::read_to_string(repo_root.join("src/app/accounts.rs")).unwrap();
    let dead_helpers = [
        "pub fn handle_accounts_move_down(",
        "pub fn handle_accounts_move_up(",
        "pub fn start_accounts_filter(",
        "pub fn accounts_filter_push_char(",
        "pub fn accounts_filter_pop_char(",
        "pub fn confirm_accounts_filter(",
        "pub fn cancel_accounts_edit(",
        "pub fn dismiss_accounts_edit(",
        "pub fn accounts_edit_push_char(",
        "pub fn accounts_edit_pop_char(",
        "pub fn accounts_edit_toggle_enabled(",
        "pub fn accounts_edit_next_field(",
        "pub fn accounts_edit_prev_field(",
        "pub fn cycle_accounts_filter(",
        "pub fn set_accounts_filter(",
        "pub fn confirm_accounts_edit(",
        "pub fn start_accounts_edit(",
    ];

    for helper in dead_helpers {
        assert!(
            !accounts.contains(helper),
            "src/app/accounts.rs should stop exposing legacy Accounts input/edit helpers once the reducer path owns those interactions: {helper}",
        );
    }
}

#[test]
fn app_accounts_bridge_no_longer_exposes_dead_legacy_side_effect_helpers() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let accounts = fs::read_to_string(repo_root.join("src/app/accounts.rs")).unwrap();
    let dead_helpers = [
        "pub fn execute_pending_confirm(",
        "pub fn delete_codex_profile(",
        "pub fn delete_claude_profile(",
        "pub fn import_codex_auth(",
        "pub fn import_claude_auth(",
        "pub fn write_selected_claude_profile_to_home(",
        "pub fn write_selected_profile_to_codex_home(",
        "pub fn reload_codex_profiles(",
        "pub fn reload_claude_profiles(",
        "pub fn update_codex_quota_for_profile(",
        "pub fn update_claude_quota_for_profile(",
        "pub fn apply_quota_refresh_result(",
    ];

    for helper in dead_helpers {
        assert!(
            !accounts.contains(helper),
            "src/app/accounts.rs should stop exposing dead legacy Accounts side-effect helpers once reducer/effect dispatch owns those workflows: {helper}",
        );
    }
}

#[test]
fn app_accounts_selected_account_uses_root_state_only() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let accounts = fs::read_to_string(repo_root.join("src/app/accounts.rs")).unwrap();

    assert!(
        accounts.contains(".accounts\n            .selected_entry()")
            || accounts.contains(".accounts\r\n            .selected_entry()"),
        "selected_account should read reducer-owned root selection state",
    );
    assert!(
        !accounts.contains("let list = build_unified_list("),
        "selected_account should stop reconstructing selection from legacy accounts_tab lists",
    );
}

#[test]
fn accounts_detail_renderer_no_longer_reads_legacy_quota_or_status_state() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let detail = fs::read_to_string(repo_root.join("src/ui/accounts/detail.rs")).unwrap();

    assert!(
        !detail.contains("state.accounts_tab.quota"),
        "Accounts detail renderer should read selected quota from root-owned accounts state",
    );
    assert!(
        !detail.contains("state.accounts_tab.status_message"),
        "Accounts detail renderer should read status messages from root-owned accounts state",
    );
}

#[test]
fn accounts_list_renderer_no_longer_reads_legacy_active_profile_state() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let list = fs::read_to_string(repo_root.join("src/ui/accounts/list.rs")).unwrap();

    assert!(
        !list.contains("state.accounts_tab.codex_active_profile_id"),
        "Accounts list renderer should read active Codex profile from root-owned accounts state",
    );
    assert!(
        !list.contains("state.accounts_tab.claude_active_profile_id"),
        "Accounts list renderer should read active Claude profile from root-owned accounts state",
    );
    assert!(
        !list.contains("state.accounts_tab.codex_quota_cache"),
        "Accounts list renderer should read quota indicators from root-owned accounts state",
    );
    assert!(
        !list.contains("state.accounts_tab.claude_quota_cache"),
        "Accounts list renderer should read quota indicators from root-owned accounts state",
    );
}

#[test]
fn accounts_bridge_no_longer_mirrors_legacy_selection_or_mode_fields() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let accounts = fs::read_to_string(repo_root.join("src/app/accounts.rs")).unwrap();

    let dead_reads = [
        "self.accounts_tab.active_filter",
        "self.accounts_tab.selected_index",
        "self.accounts_tab.codex_profile_ids",
        "self.accounts_tab.claude_profile_ids",
        "self.accounts_tab.codex_active_profile_id",
        "self.accounts_tab.claude_active_profile_id",
        "self.accounts_tab.filter_mode",
        "self.accounts_tab.filter_input",
        "self.accounts_tab.pending_confirm",
        "self.accounts_tab.start_edit(",
        "self.accounts_tab.cancel_edit(",
    ];

    for dead_read in dead_reads {
        assert!(
            !accounts.contains(dead_read),
            "Accounts bridge should stop mirroring reducer-owned selection/mode state into legacy tab fields: {dead_read}",
        );
    }
}

#[test]
fn runtime_no_longer_clears_accounts_status_through_legacy_tab_state() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let runtime = fs::read_to_string(repo_root.join("src/services/runtime/mod.rs")).unwrap();

    assert!(
        !runtime.contains("state.accounts_tab.clear_expired_status()"),
        "runtime loop should clear Accounts status through AppState/root-owned flow instead of AccountsTabState",
    );
}

#[test]
fn app_accounts_no_longer_syncs_root_from_legacy_overlay_state() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let accounts = fs::read_to_string(repo_root.join("src/app/accounts.rs")).unwrap();

    assert!(
        !accounts.contains("fn sync_root_accounts_from_legacy_tab("),
        "Accounts bridge should stop syncing root state from legacy tab state once root owns dismiss/mode flow",
    );
}
