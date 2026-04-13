use nexus::domain::system::{CodexBackendHealth, DetectorKind, DetectorNotification};

#[test]
fn detector_kind_labels_remain_stable() {
    assert_eq!(DetectorKind::Claude.as_str(), "claude");
    assert_eq!(DetectorKind::Codex.as_str(), "codex");
    assert_eq!(DetectorKind::OpenCode.as_str(), "opencode");
    assert_eq!(DetectorKind::ProcessScanner.as_str(), "process-scanner");
}

#[test]
fn detector_notification_uses_typed_detector_ids() {
    let notification = DetectorNotification {
        detector: DetectorKind::Codex,
        healthy: false,
        message: "sqlite unavailable".into(),
    };

    assert_eq!(notification.detector, DetectorKind::Codex);
    assert!(!notification.healthy);
    assert_eq!(notification.message, "sqlite unavailable");
}

#[test]
fn codex_backend_health_reports_degraded_reason() {
    let healthy = CodexBackendHealth::Healthy;
    let degraded = CodexBackendHealth::degraded("snapshot lagging");

    assert_eq!(healthy.reason(), None);
    assert_eq!(degraded.reason(), Some("snapshot lagging"));
}
