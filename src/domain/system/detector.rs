#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DetectorKind {
    Claude,
    Codex,
    OpenCode,
    ProcessScanner,
}

impl DetectorKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::OpenCode => "opencode",
            Self::ProcessScanner => "process-scanner",
        }
    }
}
