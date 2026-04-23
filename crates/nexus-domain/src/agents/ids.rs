#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AgentTool {
    OpenCode,
    Claude,
    Codex,
}

impl AgentTool {
    pub fn as_str(&self) -> &'static str {
        match self {
            AgentTool::OpenCode => "opencode",
            AgentTool::Claude => "claude",
            AgentTool::Codex => "codex",
        }
    }

    /// Whether a subagent name matches, accounting for tool-specific conventions.
    ///
    /// Codex uses the subagent description as an alternative match target because
    /// the thread spawn metadata sometimes stores the task description rather than
    /// the agent nickname.
    #[must_use]
    pub fn matches_subagent_name(
        &self,
        subagent_name: &str,
        subagent_description: Option<&str>,
        candidate: &str,
    ) -> bool {
        subagent_name == candidate
            || (*self == AgentTool::Codex && subagent_description == Some(candidate))
    }
}

impl std::fmt::Display for AgentTool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[must_use]
pub enum AgentId {
    Process { tool: AgentTool, pid: u32 },
    Session { tool: AgentTool, session_id: String },
}

impl AgentId {
    pub fn new(tool: AgentTool, pid: u32) -> Self {
        Self::Process { tool, pid }
    }

    pub fn session(tool: AgentTool, session_id: impl Into<String>) -> Self {
        Self::Session {
            tool,
            session_id: session_id.into(),
        }
    }

    pub fn tool(&self) -> AgentTool {
        match self {
            Self::Process { tool, .. } | Self::Session { tool, .. } => *tool,
        }
    }

    pub fn pid(&self) -> Option<u32> {
        match self {
            Self::Process { pid, .. } => Some(*pid),
            Self::Session { .. } => None,
        }
    }

    pub fn session_id(&self) -> Option<&str> {
        match self {
            Self::Process { .. } => None,
            Self::Session { session_id, .. } => Some(session_id.as_str()),
        }
    }

    #[must_use]
    pub fn is_process(&self) -> bool {
        matches!(self, Self::Process { .. })
    }

    #[must_use]
    pub fn is_session(&self) -> bool {
        matches!(self, Self::Session { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_subagent_name_exact_match() {
        assert!(AgentTool::Claude.matches_subagent_name("worker-1", None, "worker-1"));
    }

    #[test]
    fn matches_subagent_name_no_match() {
        assert!(!AgentTool::Claude.matches_subagent_name("worker-1", None, "worker-2"));
    }

    #[test]
    fn matches_subagent_name_codex_falls_back_to_description() {
        assert!(AgentTool::Codex.matches_subagent_name(
            "thread-abc",
            Some("fix the tests"),
            "fix the tests",
        ));
    }

    #[test]
    fn matches_subagent_name_non_codex_ignores_description() {
        assert!(!AgentTool::Claude.matches_subagent_name(
            "thread-abc",
            Some("fix the tests"),
            "fix the tests",
        ));
    }
}
