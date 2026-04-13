#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Workspace {
    #[default]
    Overview,
    Accounts,
}

impl Workspace {
    pub const ALL: [Self; 2] = [Self::Overview, Self::Accounts];

    pub fn next(self) -> Self {
        match self {
            Self::Overview => Self::Accounts,
            Self::Accounts => Self::Overview,
        }
    }

    #[cfg(test)]
    pub fn previous(self) -> Self {
        match self {
            Self::Overview => Self::Accounts,
            Self::Accounts => Self::Overview,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Overview => "Overview",
            Self::Accounts => "Accounts",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WorkspaceState {
    pub(in crate::app) active: Workspace,
}

impl WorkspaceState {
    pub fn active(&self) -> Workspace {
        self.active
    }

    #[cfg(test)]
    pub fn switch_to(&mut self, workspace: Workspace) {
        self.active = workspace;
    }

    pub fn next(&mut self) {
        self.active = self.active.next();
    }

    #[cfg(test)]
    pub fn previous(&mut self) {
        self.active = self.active.previous();
    }
}
