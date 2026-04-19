use nexus::testing::{
    extract_auth_metadata, from_epoch_millis, from_epoch_secs, from_rfc3339, load_profile_metadata,
    load_quota_cache, parse_legacy_status_output, parse_quota_from_sessions, save_profile_metadata,
    save_quota_cache, Activity, Agent, AgentDetail, AgentId, AgentIdent, AgentLifecycle, AgentTool,
    ClaudeProfileMetadata, CodexBackendHealth, Confidence, DetectorKind, DetectorNotification,
    LimitState, NormalizedReason, NormalizedState, NormalizedStatus, PrimaryStatus,
    ProfileMetadata, QuotaCache, QuotaSnapshot, SortMode, StoredCodexProfile, TodoItem,
    TodoPriority, TodoStatus,
};
use serde_json::json;
use std::fs;
use tempfile::tempdir;

#[test]
fn test_normalized_status_labels() {
    assert_eq!(NormalizedStatus::Working.as_str(), "working");
    assert_eq!(NormalizedStatus::NeedsInput.as_str(), "needs input");
    assert_eq!(NormalizedStatus::Blocked.as_str(), "blocked");
    assert_eq!(NormalizedStatus::Done.as_str(), "done");
    assert_eq!(NormalizedStatus::Unknown.as_str(), "unknown");

    assert_eq!(NormalizedReason::Subagent.as_str(), "subagent");
    assert_eq!(NormalizedReason::Review.as_str(), "review");
    assert_eq!(
        NormalizedReason::ReadyForNextTask.as_str(),
        "ready for next task"
    );
}

#[test]
fn test_normalized_status_precedence_order() {
    assert!(NormalizedStatus::NeedsInput > NormalizedStatus::Blocked);
    assert!(NormalizedStatus::Blocked > NormalizedStatus::Working);
    assert!(NormalizedStatus::Working > NormalizedStatus::Done);
    assert!(NormalizedStatus::Done > NormalizedStatus::Unknown);
}

#[test]
fn test_normalized_state_without_reason() {
    let state = NormalizedState::new(NormalizedStatus::Working, None);

    assert_eq!(state.status, NormalizedStatus::Working);
    assert_eq!(state.reason, None);
}

#[test]
fn test_normalized_state_with_reason() {
    let state = NormalizedState::new(
        NormalizedStatus::Done,
        Some(NormalizedReason::ReadyForNextTask),
    );

    assert_eq!(state.status, NormalizedStatus::Done);
    assert_eq!(state.reason, Some(NormalizedReason::ReadyForNextTask));
    assert_eq!(
        state.reason.as_ref().map(|reason| reason.as_str()),
        Some("ready for next task")
    );
}

#[test]
fn test_primary_status_ordering() {
    let mut statuses = vec![
        PrimaryStatus::Done,
        PrimaryStatus::Running,
        PrimaryStatus::Waiting,
        PrimaryStatus::Exited,
        PrimaryStatus::Error,
    ];
    statuses.sort();
    assert_eq!(
        statuses,
        vec![
            PrimaryStatus::Waiting,
            PrimaryStatus::Running,
            PrimaryStatus::Error,
            PrimaryStatus::Done,
            PrimaryStatus::Exited,
        ]
    );
}

#[test]
fn test_primary_status_display() {
    assert_eq!(PrimaryStatus::Waiting.as_str(), "WAITING");
    assert_eq!(PrimaryStatus::Running.as_str(), "RUNNING");
    assert_eq!(PrimaryStatus::Error.as_str(), "ERROR");
    assert_eq!(PrimaryStatus::Done.as_str(), "DONE");
    assert_eq!(PrimaryStatus::Exited.as_str(), "EXITED");
}

#[test]
fn test_agent_tool_display() {
    assert_eq!(AgentTool::OpenCode.as_str(), "opencode");
    assert_eq!(AgentTool::Claude.as_str(), "claude");
    assert_eq!(AgentTool::Codex.as_str(), "codex");
}

#[test]
fn test_agent_id_uniqueness() {
    let id1 = AgentId::new(AgentTool::OpenCode, 1234);
    let id2 = AgentId::new(AgentTool::OpenCode, 5678);
    let id3 = AgentId::new(AgentTool::Claude, 1234);
    assert_ne!(id1, id2);
    assert_ne!(id1, id3);
}

#[test]
fn test_agent_id_process_and_session_are_distinct() {
    let process = AgentId::new(AgentTool::Codex, 1234);
    let session = AgentId::session(AgentTool::Codex, "session-a");

    assert_ne!(process, session);
    assert!(process.is_process());
    assert!(!process.is_session());
    assert!(session.is_session());
    assert!(!session.is_process());
    assert_eq!(process.tool(), AgentTool::Codex);
    assert_eq!(session.tool(), AgentTool::Codex);
}

#[test]
fn test_agent_elapsed_time() {
    let agent = Agent::new(
        AgentIdent::new(
            AgentId::new(AgentTool::Claude, 1000),
            Some(1000),
            None,
            None,
            Some(std::path::PathBuf::from("/tmp/test")),
            Some("main".to_string()),
            false,
            chrono::Local::now() - chrono::Duration::minutes(5),
        ),
        AgentLifecycle::new(
            PrimaryStatus::Running,
            NormalizedState::new(NormalizedStatus::Working, None),
            Some(Activity::Generating),
            chrono::Local::now(),
        ),
        AgentDetail::new(vec![], std::collections::VecDeque::new(), Vec::new()),
    );
    let elapsed = agent.elapsed();
    assert!(elapsed >= chrono::Duration::minutes(4));
    assert!(elapsed <= chrono::Duration::minutes(6));
}

// ── Timestamp conversion helper tests ─────────────────────────────

#[test]
fn test_from_epoch_millis_zero_returns_none() {
    assert!(from_epoch_millis(0).is_none());
}

#[test]
fn test_from_epoch_millis_valid() {
    // 2026-01-01T00:00:00Z = 1767225600000 ms
    let result = from_epoch_millis(1_767_225_600_000);
    assert!(result.is_some());
}

#[test]
fn test_from_epoch_millis_negative_is_valid() {
    // Negative timestamps (before 1970) are valid
    let result = from_epoch_millis(-1);
    assert!(result.is_some());
}

#[test]
fn test_from_epoch_secs_zero_returns_none() {
    assert!(from_epoch_secs(0).is_none());
}

#[test]
fn test_from_epoch_secs_valid() {
    // 2026-01-01T00:00:00Z = 1767225600 sec
    let result = from_epoch_secs(1_767_225_600);
    assert!(result.is_some());
}

#[test]
fn test_from_rfc3339_valid() {
    let result = from_rfc3339("2026-03-31T10:00:00Z");
    assert!(result.is_some());
}

#[test]
fn test_from_rfc3339_invalid_returns_none() {
    assert!(from_rfc3339("not-a-date").is_none());
    assert!(from_rfc3339("").is_none());
}

#[test]
fn test_activity_compacting_as_str() {
    assert_eq!(Activity::Compacting.as_str(), "compacting");
    assert_eq!(format!("{}", Activity::Compacting), "compacting");
}

#[test]
fn test_display_name_without_alias() {
    let agent = Agent::new(
        AgentIdent::new(
            AgentId::new(AgentTool::Claude, 100),
            Some(100),
            None,
            None,
            Some(std::path::PathBuf::from("/tmp")),
            None,
            false,
            chrono::Local::now(),
        ),
        AgentLifecycle::new(
            PrimaryStatus::Running,
            NormalizedState::new(NormalizedStatus::Working, None),
            None,
            chrono::Local::now(),
        ),
        AgentDetail::new(Vec::new(), std::collections::VecDeque::new(), Vec::new()),
    );
    assert_eq!(agent.display_name(), "claude");
}

#[test]
fn test_display_name_with_alias() {
    let agent = Agent::new(
        AgentIdent::new(
            AgentId::new(AgentTool::OpenCode, 200),
            Some(200),
            None,
            Some("my-agent".to_string()),
            Some(std::path::PathBuf::from("/tmp")),
            None,
            false,
            chrono::Local::now(),
        ),
        AgentLifecycle::new(
            PrimaryStatus::Running,
            NormalizedState::new(NormalizedStatus::Working, None),
            None,
            chrono::Local::now(),
        ),
        AgentDetail::new(Vec::new(), std::collections::VecDeque::new(), Vec::new()),
    );
    assert_eq!(agent.display_name(), "my-agent");
}

#[test]
fn test_sort_mode_cycling() {
    assert_eq!(SortMode::Urgency.next(), SortMode::Name);
    assert_eq!(SortMode::Name.next(), SortMode::Elapsed);
    assert_eq!(SortMode::Elapsed.next(), SortMode::Urgency);
}

#[test]
fn test_todo_status_equality() {
    assert_eq!(TodoStatus::Pending, TodoStatus::Pending);
    assert_ne!(TodoStatus::Pending, TodoStatus::InProgress);
    assert_ne!(TodoStatus::Completed, TodoStatus::Cancelled);
}

#[test]
fn test_todo_priority_equality() {
    assert_eq!(TodoPriority::High, TodoPriority::High);
    assert_ne!(TodoPriority::High, TodoPriority::Low);
}

#[test]
fn test_todo_item_equality() {
    let a = TodoItem {
        content: "Fix bug".to_string(),
        status: TodoStatus::Pending,
        priority: TodoPriority::High,
    };
    let b = TodoItem {
        content: "Fix bug".to_string(),
        status: TodoStatus::Pending,
        priority: TodoPriority::High,
    };
    let c = TodoItem {
        content: "Fix bug".to_string(),
        status: TodoStatus::Completed,
        priority: TodoPriority::High,
    };
    assert_eq!(a, b);
    assert_ne!(a, c);
}

#[test]
fn test_todo_status_from_str() {
    assert_eq!(TodoStatus::parse_str("pending"), TodoStatus::Pending);
    assert_eq!(TodoStatus::parse_str("in_progress"), TodoStatus::InProgress);
    assert_eq!(TodoStatus::parse_str("completed"), TodoStatus::Completed);
    assert_eq!(TodoStatus::parse_str("cancelled"), TodoStatus::Cancelled);
    assert_eq!(TodoStatus::parse_str("unknown_value"), TodoStatus::Pending);
}

#[test]
fn test_todo_priority_from_str() {
    assert_eq!(TodoPriority::parse_str("high"), TodoPriority::High);
    assert_eq!(TodoPriority::parse_str("medium"), TodoPriority::Medium);
    assert_eq!(TodoPriority::parse_str("low"), TodoPriority::Low);
    assert_eq!(
        TodoPriority::parse_str("unknown_value"),
        TodoPriority::Medium
    );
}

#[test]
fn codex_profile_extracts_email_and_plan() {
    let metadata = extract_auth_metadata(&json!({
        "email": "dev@example.com",
        "subscription": "Pro"
    }));

    assert_eq!(metadata.email.as_deref(), Some("dev@example.com"));
    assert_eq!(metadata.plan.as_deref(), Some("Pro"));
}

#[test]
fn codex_profile_imports_auth_and_sets_defaults() {
    let temp = tempdir().unwrap();
    let source = temp.path().join("auth.json");
    let profile_dir = temp.path().join("profiles").join("codex");
    let auth_json = r#"{"email":"dev@example.com","subscription":"Pro"}"#;

    fs::write(&source, auth_json).unwrap();

    let profile =
        StoredCodexProfile::import_from_auth("profile-1", "Primary", &profile_dir, &source)
            .unwrap();

    assert_eq!(profile.id, "profile-1");
    assert_eq!(profile.label, "Primary");
    assert_eq!(profile.email.as_deref(), Some("dev@example.com"));
    assert_eq!(profile.plan.as_deref(), Some("Pro"));
    assert_eq!(profile.notes, None);
    assert!(profile.enabled);
    assert_eq!(profile.last_used, None);
    assert!(profile_dir.exists());
    assert!(profile_dir.join("auth.json").is_file());
    assert_eq!(
        fs::read_to_string(profile_dir.join("auth.json")).unwrap(),
        auth_json
    );
    assert_eq!(profile.created_at, profile.updated_at);
}

#[test]
fn codex_profile_import_rejects_invalid_auth_without_partial_state() {
    let temp = tempdir().unwrap();
    let source = temp.path().join("auth.json");
    let profile_dir = temp.path().join("profiles").join("codex");

    fs::write(&source, "{invalid json").unwrap();

    let result =
        StoredCodexProfile::import_from_auth("profile-1", "Primary", &profile_dir, &source);

    assert!(result.is_err());
    assert!(!profile_dir.exists());
}

#[test]
fn codex_profile_import_rejects_blank_id_or_label() {
    let temp = tempdir().unwrap();
    let source = temp.path().join("auth.json");
    let profile_dir = temp.path().join("profiles").join("codex");

    fs::write(&source, r#"{"email":"dev@example.com"}"#).unwrap();

    let blank_id = StoredCodexProfile::import_from_auth("   ", "Primary", &profile_dir, &source);
    let blank_label =
        StoredCodexProfile::import_from_auth("profile-1", "\n\t", &profile_dir, &source);

    assert!(blank_id.is_err());
    assert!(blank_label.is_err());
    assert!(!profile_dir.exists());
}

#[test]
fn codex_quota_parser_marks_unknown_when_state_is_unclear() {
    let snapshot = parse_legacy_status_output("5h window: 42% used").expect("snapshot");

    assert_eq!(snapshot.confidence, Confidence::Parsed);
    assert_eq!(snapshot.five_hour_state, LimitState::Unknown);
}

#[test]
fn codex_quota_parser_returns_none_when_no_quota_lines_exist() {
    assert!(parse_legacy_status_output("status: idle").is_none());
}

#[test]
fn codex_quota_parser_parses_both_windows_with_reset_timestamps() {
    let snapshot = parse_legacy_status_output(
        "5h window: available, resets at 2026-04-03T12:34:56Z\nweekly window: not reached, resets at 2026-04-10T08:00:00Z",
    )
    .expect("snapshot");

    assert_eq!(snapshot.confidence, Confidence::Parsed);
    assert_eq!(snapshot.five_hour_state, LimitState::Available);
    assert_eq!(snapshot.weekly_state, LimitState::Available);
    assert_eq!(
        snapshot
            .five_hour_reset_at
            .as_ref()
            .map(|dt| dt.to_rfc3339()),
        Some("2026-04-03T12:34:56+00:00".to_string())
    );
    assert_eq!(
        snapshot.weekly_reset_at.as_ref().map(|dt| dt.to_rfc3339()),
        Some("2026-04-10T08:00:00+00:00".to_string())
    );
}

#[test]
fn codex_quota_cache_save_load_round_trip() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("quota-cache.json");
    let snapshot = parse_legacy_status_output(
        "5h window: available, resets at 2026-04-03T12:34:56Z\nweekly window: not reached, resets at 2026-04-10T08:00:00Z",
    )
    .expect("snapshot");

    let mut cache = QuotaCache::default();
    cache.by_profile.insert("profile-1".to_string(), snapshot);

    save_quota_cache(&cache, &path).unwrap();
    let loaded = load_quota_cache(&path).unwrap();

    assert_eq!(loaded, cache);
}

#[test]
fn codex_quota_cache_loads_malformed_file_as_error() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("quota-cache.json");

    fs::write(&path, "{ invalid json").unwrap();

    let err = load_quota_cache(&path).unwrap_err();

    assert!(err.to_string().contains("failed to parse"));
    assert!(err.to_string().contains("quota-cache.json"));
}

#[test]
fn codex_quota_snapshot_old_json_without_pct_fields_deserializes_ok() {
    // JSON produced before the pct fields existed — must deserialize without error
    // and leave the new fields as None.
    let json = r#"{
        "captured_at": "2026-04-03T12:00:00Z",
        "confidence": "Parsed",
        "five_hour_state": "Available",
        "five_hour_reset_at": null,
        "weekly_state": "Unknown",
        "weekly_reset_at": null,
        "summary": "5h=Available; weekly=Unknown"
    }"#;
    let snapshot: QuotaSnapshot = serde_json::from_str(json).expect("deserialization failed");
    assert_eq!(snapshot.five_hour_used_pct, None);
    assert_eq!(snapshot.weekly_used_pct, None);
    assert_eq!(snapshot.five_hour_state, LimitState::Available);
}

#[test]
fn parse_quota_from_sessions_returns_none_for_missing_dir() {
    let result = parse_quota_from_sessions(std::path::Path::new("/nonexistent/sessions/dir"));
    assert!(result.is_none());
}

#[test]
fn parse_quota_from_sessions_returns_none_for_empty_dir() {
    let temp = tempdir().unwrap();
    let result = parse_quota_from_sessions(temp.path());
    assert!(result.is_none());
}

#[test]
fn parse_quota_from_sessions_returns_none_when_no_token_count_events() {
    let temp = tempdir().unwrap();
    let day_dir = temp.path().join("2026").join("04").join("04");
    fs::create_dir_all(&day_dir).unwrap();
    fs::write(
        day_dir.join("rollout-test.jsonl"),
        r#"{"type":"session_meta","payload":{"id":"abc"}}"#,
    )
    .unwrap();
    let result = parse_quota_from_sessions(temp.path());
    assert!(result.is_none());
}

#[test]
fn parse_quota_from_sessions_extracts_pct_and_resets_at() {
    let temp = tempdir().unwrap();
    let day_dir = temp.path().join("2026").join("04").join("04");
    fs::create_dir_all(&day_dir).unwrap();

    // resets_at values far in the future so the snapshot isn't considered stale
    let content = r#"{"type":"event_msg","payload":{"type":"token_count","info":{},"rate_limits":{"primary":{"used_percent":47.0,"window_minutes":300,"resets_at":9999999999},"secondary":{"used_percent":85.0,"window_minutes":10080,"resets_at":9999999999},"plan_type":"plus","credits":null}}}"#;
    fs::write(day_dir.join("rollout-test.jsonl"), content).unwrap();

    let snapshot = parse_quota_from_sessions(temp.path()).expect("snapshot expected");
    assert_eq!(snapshot.five_hour_used_pct, Some(47));
    assert_eq!(snapshot.weekly_used_pct, Some(85));
    assert_eq!(snapshot.five_hour_state, LimitState::Available);
    assert_eq!(snapshot.weekly_state, LimitState::Available);
    assert_eq!(snapshot.confidence, Confidence::Parsed);
    assert!(snapshot.five_hour_reset_at.is_some());
    assert!(snapshot.weekly_reset_at.is_some());
}

#[test]
fn parse_quota_from_sessions_derives_hit_state_at_100_pct() {
    let temp = tempdir().unwrap();
    let day_dir = temp.path().join("2026").join("04").join("04");
    fs::create_dir_all(&day_dir).unwrap();

    let content = r#"{"type":"event_msg","payload":{"type":"token_count","info":{},"rate_limits":{"primary":{"used_percent":100.0,"window_minutes":300,"resets_at":9999999999},"secondary":{"used_percent":100.0,"window_minutes":10080,"resets_at":9999999999},"credits":null}}}"#;
    fs::write(day_dir.join("rollout-test.jsonl"), content).unwrap();

    let snapshot = parse_quota_from_sessions(temp.path()).expect("snapshot expected");
    assert_eq!(snapshot.five_hour_used_pct, Some(100));
    assert_eq!(snapshot.five_hour_state, LimitState::Hit);
    assert_eq!(snapshot.weekly_state, LimitState::Hit);
}

#[test]
fn parse_quota_from_sessions_picks_last_token_count_event_in_file() {
    let temp = tempdir().unwrap();
    let day_dir = temp.path().join("2026").join("04").join("04");
    fs::create_dir_all(&day_dir).unwrap();

    // First event: 10%, second event: 60% — should return 60%
    let content = concat!(
        r#"{"type":"event_msg","payload":{"type":"token_count","info":{},"rate_limits":{"primary":{"used_percent":10.0,"window_minutes":300,"resets_at":9999999999},"secondary":{"used_percent":10.0,"window_minutes":10080,"resets_at":9999999999},"credits":null}}}"#,
        "\n",
        r#"{"type":"event_msg","payload":{"type":"token_count","info":{},"rate_limits":{"primary":{"used_percent":60.0,"window_minutes":300,"resets_at":9999999999},"secondary":{"used_percent":60.0,"window_minutes":10080,"resets_at":9999999999},"credits":null}}}"#,
    );
    fs::write(day_dir.join("rollout-test.jsonl"), content).unwrap();

    let snapshot = parse_quota_from_sessions(temp.path()).expect("snapshot expected");
    assert_eq!(snapshot.five_hour_used_pct, Some(60));
}

#[test]
fn profile_metadata_save_load_round_trip() {
    let temp_dir = tempfile::tempdir().unwrap();
    let profile_dir = temp_dir.path().to_path_buf();

    let metadata = ProfileMetadata {
        id: "test-profile".to_string(),
        label: Some("My Label".to_string()),
        email: Some("test@example.com".to_string()),
        plan: Some("Pro".to_string()),
        notes: Some("Some notes".to_string()),
        enabled: false,
        created_at: None,
        updated_at: None,
    };

    save_profile_metadata(&profile_dir, &metadata).unwrap();
    let loaded = load_profile_metadata(&profile_dir).unwrap();

    assert_eq!(loaded.label, metadata.label);
    assert_eq!(loaded.notes, metadata.notes);
    assert_eq!(loaded.enabled, metadata.enabled);
}

#[test]
fn test_agent_model_preserves_alias_elapsed_and_runtime_state() {
    use std::collections::VecDeque;
    use std::path::PathBuf;

    let started_at = chrono::Local::now() - chrono::Duration::minutes(5);
    let agent = Agent::new(
        AgentIdent::new(
            AgentId::new(AgentTool::Claude, 100),
            Some(100),
            Some("thread-1".into()),
            Some("review-me".into()),
            Some(PathBuf::from("/tmp/repo")),
            Some("main".into()),
            false,
            started_at,
        ),
        AgentLifecycle::new(
            PrimaryStatus::Running,
            NormalizedState::new(NormalizedStatus::NeedsInput, Some(NormalizedReason::Review)),
            Some(Activity::Generating),
            chrono::Local::now(),
        ),
        AgentDetail::new(Vec::new(), VecDeque::new(), Vec::new()),
    );

    assert_eq!(agent.display_name(), "review-me");
    assert_eq!(agent.status().as_str(), "RUNNING");
    assert_eq!(agent.normalized().reason, Some(NormalizedReason::Review));
    assert!(agent.elapsed() >= chrono::Duration::minutes(4));
}

#[test]
fn test_quota_legacy_fallback_keeps_window_and_summary_views_in_sync() {
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
fn test_detector_domain_types_encode_health_without_stringly_identifiers() {
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
fn test_metadata_defaults_keep_profiles_enabled_until_explicitly_disabled() {
    let codex = ProfileMetadata::default();
    let claude = ClaudeProfileMetadata::default();

    assert!(codex.enabled);
    assert!(claude.enabled);
    assert_eq!(codex.label, None);
    assert_eq!(claude.label, None);
    assert_eq!(codex.id, "");
    assert_eq!(claude.id, "");
}
