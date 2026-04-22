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

impl std::fmt::Display for DetectorKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Claude => f.write_str("Claude"),
            Self::Codex => f.write_str("Codex"),
            Self::OpenCode => f.write_str("OpenCode"),
            Self::ProcessScanner => f.write_str("ProcessScanner"),
        }
    }
}
