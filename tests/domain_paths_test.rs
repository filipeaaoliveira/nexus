use std::collections::VecDeque;
use std::path::PathBuf;

use nexus::claude_profiles::profile::ClaudeProfileMetadata;
use nexus::codex_profiles::profile::ProfileMetadata;
use nexus::codex_profiles::quota::{Confidence, LimitState, QuotaSnapshot};
use nexus::domain::agents::activity::Activity;
use nexus::domain::agents::ids::{AgentId, AgentTool};
use nexus::domain::agents::model::Agent;
use nexus::domain::agents::status::{
    NormalizedReason, NormalizedState, NormalizedStatus, PrimaryStatus,
};
use nexus::domain::shared::time::{
    format_duration, format_duration_short, from_epoch_millis, from_epoch_secs, from_rfc3339,
};
use nexus::domain::system::health::CodexBackendHealth;
use nexus::domain::system::notification::DetectorNotification;
use nexus::domain::system::DetectorKind;

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
        cwd: Some(PathBuf::from("/tmp/repo")),
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
