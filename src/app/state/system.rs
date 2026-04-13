use std::collections::BTreeMap;

use crate::domain::accounts::{ProfileId, Provider};
use crate::domain::system::DetectorKind;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetectorHealth {
    pub healthy: bool,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Overlay {
    #[default]
    None,
    Help {
        scroll: u16,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Workflow {
    #[default]
    Idle,
    #[cfg(test)]
    LoadingProfiles,
    SavingProfile {
        provider: Provider,
        profile_id: ProfileId,
    },
    WritingProfileToHome {
        provider: Provider,
        profile_id: ProfileId,
    },
    DeletingProfile {
        provider: Provider,
        profile_id: ProfileId,
    },
    Quitting,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SystemState {
    pub(in crate::app) overlay: Overlay,
    pub(in crate::app) workflow: Workflow,
    pub(in crate::app) detector_health: BTreeMap<DetectorKind, DetectorHealth>,
}

impl SystemState {
    pub fn overlay(&self) -> &Overlay {
        &self.overlay
    }

    pub fn detector_health(&self) -> &BTreeMap<DetectorKind, DetectorHealth> {
        &self.detector_health
    }

    #[cfg(test)]
    pub fn show_overlay(&mut self, overlay: Overlay) {
        self.overlay = overlay;
    }

    pub fn clear_overlay(&mut self) {
        self.overlay = Overlay::None;
    }

    pub fn record_detector_health(
        &mut self,
        detector: DetectorKind,
        healthy: bool,
        message: impl Into<String>,
    ) {
        self.detector_health.insert(
            detector,
            DetectorHealth {
                healthy,
                message: message.into(),
            },
        );
    }
}
