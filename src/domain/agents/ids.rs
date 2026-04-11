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
}

impl std::fmt::Display for AgentTool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AgentIdentity {
    Process { tool: AgentTool, pid: u32 },
    Session { tool: AgentTool, session_id: String },
}

impl AgentIdentity {
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

    pub fn is_process(&self) -> bool {
        matches!(self, Self::Process { .. })
    }

    pub fn is_session(&self) -> bool {
        matches!(self, Self::Session { .. })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AgentId {
    pub identity: AgentIdentity,
}

impl AgentId {
    pub fn new(tool: AgentTool, pid: u32) -> Self {
        Self {
            identity: AgentIdentity::Process { tool, pid },
        }
    }

    pub fn session(tool: AgentTool, session_id: impl Into<String>) -> Self {
        Self {
            identity: AgentIdentity::Session {
                tool,
                session_id: session_id.into(),
            },
        }
    }

    pub fn tool(&self) -> AgentTool {
        self.identity.tool()
    }

    pub fn pid(&self) -> Option<u32> {
        self.identity.pid()
    }

    pub fn session_id(&self) -> Option<&str> {
        self.identity.session_id()
    }

    pub fn is_process(&self) -> bool {
        self.identity.is_process()
    }

    pub fn is_session(&self) -> bool {
        self.identity.is_session()
    }
}
