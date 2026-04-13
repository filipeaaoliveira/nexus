use crate::domain::agents::ids::AgentId;
use crate::domain::agents::model::SortMode;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum OverviewMode {
    #[default]
    Normal,
    Filtering {
        input: String,
    },
    Renaming {
        agent_id: AgentId,
        input: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OverviewState {
    pub(in crate::app) mode: OverviewMode,
    pub(in crate::app) selected_index: Option<usize>,
    pub(in crate::app) sort_mode: SortMode,
    pub(in crate::app) filter: Option<String>,
}

impl Default for OverviewState {
    fn default() -> Self {
        Self {
            mode: OverviewMode::Normal,
            selected_index: None,
            sort_mode: SortMode::Name,
            filter: None,
        }
    }
}

impl OverviewState {
    pub fn mode(&self) -> &OverviewMode {
        &self.mode
    }

    pub fn selected_index(&self) -> Option<usize> {
        self.selected_index
    }

    pub fn sort_mode(&self) -> SortMode {
        self.sort_mode
    }

    pub fn clamp_selection(&mut self, visible_count: usize) {
        if visible_count == 0 {
            self.selected_index = None;
        } else if self.selected_index.is_none() {
            self.selected_index = Some(0);
        } else if let Some(idx) = self.selected_index {
            if idx >= visible_count {
                self.selected_index = Some(visible_count - 1);
            }
        }
    }
}
