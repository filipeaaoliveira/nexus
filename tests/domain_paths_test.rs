use std::collections::VecDeque;
use std::fs;
use std::path::{Path, PathBuf};

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
fn canonical_domain_reexports_match_their_underlying_types() {
    assert_eq!(
        std::any::TypeId::of::<Provider>(),
        std::any::TypeId::of::<DomainProvider>()
    );
    assert_eq!(
        std::any::TypeId::of::<AccountEntry>(),
        std::any::TypeId::of::<DomainAccountEntry>()
    );
    assert_eq!(
        std::any::TypeId::of::<Agent>(),
        std::any::TypeId::of::<DomainAgent>()
    );
    assert_eq!(
        std::any::TypeId::of::<AgentTool>(),
        std::any::TypeId::of::<DomainAgentTool>()
    );
    assert_eq!(
        std::any::TypeId::of::<CodexBackendHealth>(),
        std::any::TypeId::of::<DomainCodexBackendHealth>()
    );
    assert_eq!(
        std::any::TypeId::of::<DetectorNotification>(),
        std::any::TypeId::of::<DomainDetectorNotification>()
    );
}

#[test]
fn agent_identity_semantics_distinguish_process_and_session_rows() {
    let process = AgentId::new(AgentTool::Codex, 44);
    let session = AgentId::session(AgentTool::Codex, "thread-1");

    assert_eq!(process.tool(), AgentTool::Codex);
    assert!(process.is_process());
    assert!(!process.is_session());
    assert_eq!(process.pid(), Some(44));

    assert_eq!(session.tool(), AgentTool::Codex);
    assert!(session.is_session());
    assert!(!session.is_process());
    assert_eq!(session.session_id(), Some("thread-1"));
}

#[test]
fn agent_model_preserves_alias_elapsed_and_runtime_state() {
    let started_at = chrono::Local::now() - chrono::Duration::minutes(5);
    let agent = Agent {
        id: AgentId::new(AgentTool::Claude, 100),
        pid: Some(100),
        status: PrimaryStatus::Running,
        normalized: NormalizedState::new(
            NormalizedStatus::NeedsInput,
            Some(NormalizedReason::Review),
        ),
        activity: Some(Activity::Generating),
        cwd: PathBuf::from("/tmp/repo"),
        branch: Some("main".into()),
        is_worktree: false,
        started_at,
        last_activity: chrono::Local::now(),
        subagents: Vec::new(),
        recent_events: VecDeque::new(),
        session_id: Some("thread-1".into()),
        alias: Some("review-me".into()),
        todos: Vec::new(),
    };

    assert_eq!(agent.display_name(), "review-me");
    assert_eq!(agent.status.as_str(), "RUNNING");
    assert_eq!(agent.normalized.reason, Some(NormalizedReason::Review));
    assert!(agent.elapsed() >= chrono::Duration::minutes(4));
}

#[test]
fn quota_legacy_fallback_keeps_window_and_summary_views_in_sync() {
    let snapshot = QuotaSnapshot::legacy_fallback(
        chrono::Utc::now(),
        Confidence::Exact,
        LimitState::Available,
        None,
        Some(25),
        LimitState::Hit,
        None,
        Some(90),
        "25% 5h, 90% weekly".into(),
    );

    assert_eq!(snapshot.summary, "25% 5h, 90% weekly");
    assert_eq!(snapshot.confidence, Confidence::Exact);
    assert_eq!(snapshot.five_hour_state, LimitState::Available);
    assert_eq!(snapshot.five_hour_used_pct, Some(25));
    assert_eq!(snapshot.weekly_state, LimitState::Hit);
    assert_eq!(snapshot.weekly_used_pct, Some(90));
    assert_eq!(snapshot.windows.len(), 2);
    assert_eq!(snapshot.windows[0].label, "5h");
    assert_eq!(snapshot.windows[1].label, "weekly");
}

#[test]
fn detector_domain_types_encode_health_without_stringly_identifiers() {
    let healthy = CodexBackendHealth::Healthy;
    let degraded = CodexBackendHealth::degraded("backend lagging");
    let notification = DetectorNotification {
        detector: DetectorKind::ProcessScanner,
        healthy: false,
        message: "process scan stalled".into(),
    };

    assert_eq!(healthy.reason(), None);
    assert_eq!(degraded.reason(), Some("backend lagging"));
    assert_eq!(notification.detector, DetectorKind::ProcessScanner);
    assert_eq!(notification.detector.as_str(), "process-scanner");
}

#[test]
fn metadata_defaults_keep_profiles_enabled_until_explicitly_disabled() {
    let codex = ProfileMetadata::default();
    let claude = ClaudeProfileMetadata::default();

    assert!(codex.enabled);
    assert!(claude.enabled);
    assert_eq!(codex.label, None);
    assert_eq!(claude.label, None);
    assert_eq!(codex.id, "");
    assert_eq!(claude.id, "");
}

#[test]
fn time_helpers_parse_valid_inputs_and_reject_zero_epoch() {
    assert!(from_epoch_millis(0).is_none());
    assert!(from_epoch_secs(0).is_none());
    assert!(from_epoch_millis(1_767_225_600_000).is_some());
    assert!(from_epoch_secs(1_767_225_600).is_some());
    assert!(from_rfc3339("2026-03-31T10:00:00Z").is_some());
    assert!(from_rfc3339("not-a-date").is_none());
    assert!(!format_duration(chrono::Duration::minutes(5)).is_empty());
    assert!(!format_duration_short(chrono::Duration::minutes(5)).is_empty());
}

#[test]
fn representative_modules_no_longer_depend_on_transitional_types_shim() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let files = [
        "src/event.rs",
        "src/app/action.rs",
        "src/detect/process.rs",
        "src/ui/theme.rs",
    ];

    for file in files {
        let content = fs::read_to_string(repo_root.join(file)).unwrap();
        assert!(
            !content.contains("crate::types"),
            "{file} should import domain modules directly instead of the transitional types shim",
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
fn accounts_ui_no_longer_reads_legacy_accounts_tab_state() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let detail = fs::read_to_string(repo_root.join("src/ui/accounts/detail.rs")).unwrap();
    let list = fs::read_to_string(repo_root.join("src/ui/accounts/list.rs")).unwrap();

    assert!(!detail.contains("state.accounts_tab."));
    assert!(!list.contains("state.accounts_tab."));
}
