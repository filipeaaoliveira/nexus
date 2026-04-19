#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum CodexBackendHealth {
    #[default]
    Healthy,
    Degraded {
        reason: String,
    },
}

impl CodexBackendHealth {
    pub fn degraded(reason: impl Into<String>) -> Self {
        Self::Degraded {
            reason: reason.into(),
        }
    }

    pub fn reason(&self) -> Option<&str> {
        match self {
            Self::Healthy => None,
            Self::Degraded { reason } => Some(reason.as_str()),
        }
    }
}
