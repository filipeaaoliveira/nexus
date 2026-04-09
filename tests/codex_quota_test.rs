use chrono::Utc;
use nexus::codex_profiles::quota::{
    parse_legacy_status_output, parse_quota_from_sessions, Confidence, CreditsState, LimitState,
    QuotaSnapshot, QuotaWindow,
};
use std::fs;
use tempfile::tempdir;

#[test]
fn quota_snapshot_supports_structured_windows_and_credits() {
    let snapshot = QuotaSnapshot::from_windows(
        Utc::now(),
        Confidence::Parsed,
        vec![
            QuotaWindow {
                label: "15m".to_string(),
                used_pct: Some(25),
                reset_at: None,
                state: LimitState::Available,
            },
            QuotaWindow {
                label: "7d".to_string(),
                used_pct: Some(80),
                reset_at: None,
                state: LimitState::Available,
            },
        ],
        Some(CreditsState {
            enabled: true,
            balance_label: Some("$12.50".to_string()),
            unlimited: false,
        }),
        "15m=25%; 7d=80%; credits=$12.50".to_string(),
    );

    assert_eq!(snapshot.windows.len(), 2);
    assert_eq!(snapshot.windows[0].label, "15m");
    assert!(snapshot.credits.as_ref().unwrap().enabled);
}

#[test]
fn parse_quota_from_sessions_preserves_generic_windows_and_credits() {
    let temp = tempdir().expect("tempdir");
    let day_dir = temp.path().join("2026").join("04").join("08");
    fs::create_dir_all(&day_dir).expect("day dir");
    fs::write(
        day_dir.join("rollout.jsonl"),
        r#"{"type":"event_msg","payload":{"type":"token_count","rate_limits":{"windows":[{"label":"15m","usedPercent":25,"windowDurationMins":15,"resetsAt":9999999999},{"label":"7d","usedPercent":80,"windowDurationMins":10080,"resetsAt":9999999999}],"credits":{"enabled":true,"balanceLabel":"$12.50","unlimited":false}}}}"#,
    )
    .expect("rollout");

    let snapshot = parse_quota_from_sessions(temp.path()).expect("snapshot");

    assert_eq!(snapshot.windows.len(), 2);
    assert_eq!(snapshot.windows[0].label, "15m");
    assert_eq!(snapshot.windows[1].label, "7d");
    assert_eq!(
        snapshot.credits,
        Some(CreditsState {
            enabled: true,
            balance_label: Some("$12.50".to_string()),
            unlimited: false,
        })
    );
}

#[test]
fn legacy_status_parser_remains_available_via_explicit_fallback_name() {
    let snapshot = parse_legacy_status_output(
        "5h window: available, resets at 2026-04-03T12:34:56Z\nweekly window: not reached, resets at 2026-04-10T08:00:00Z",
    )
    .expect("snapshot");

    assert_eq!(snapshot.windows.len(), 2);
    assert_eq!(snapshot.windows[0].label, "5h");
    assert_eq!(snapshot.windows[1].label, "weekly");
}
