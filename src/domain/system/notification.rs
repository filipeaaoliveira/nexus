use super::DetectorKind;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetectorNotification {
    pub detector: DetectorKind,
    pub healthy: bool,
    pub message: String,
}
