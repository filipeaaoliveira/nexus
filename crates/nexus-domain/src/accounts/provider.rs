#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Provider {
    Codex,
    Claude,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum ProviderFilter {
    #[default]
    All,
    Codex,
    Claude,
}

impl ProviderFilter {
    pub fn next(self) -> Self {
        match self {
            Self::All => Self::Codex,
            Self::Codex => Self::Claude,
            Self::Claude => Self::All,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Codex => "codex",
            Self::Claude => "claude",
        }
    }
}
