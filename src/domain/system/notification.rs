#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetectorNotification {
    pub detector: String,
    pub healthy: bool,
    pub message: String,
}
