use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(Uuid);

impl SessionId {
    pub fn generate() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    pub fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

impl Default for SessionId {
    fn default() -> Self {
        Self::generate()
    }
}

impl std::fmt::Display for SessionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionState {
    Idle,
    Processing,
    WaitingApproval,
    Completed,
    Failed,
    Interrupted,
}

impl SessionState {
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Processing | Self::WaitingApproval)
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Interrupted)
    }

    pub fn can_transition_to(&self, target: SessionState) -> bool {
        match (self, target) {
            (Self::Idle, Self::Processing) => true,
            (Self::Processing, Self::WaitingApproval) => true,
            (Self::Processing, Self::Idle) => true,
            (Self::Processing, Self::Completed) => true,
            (Self::Processing, Self::Failed) => true,
            (Self::Processing, Self::Interrupted) => true,
            (Self::WaitingApproval, Self::Processing) => true,
            (Self::WaitingApproval, Self::Interrupted) => true,
            _ => false,
        }
    }
}

impl Default for SessionState {
    fn default() -> Self {
        Self::Idle
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionConfig {
    pub model_id: String,
    pub provider_id: String,
    pub timeout_ms: u64,
    pub approval_mode: ApprovalMode,
    pub max_turns: u32,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            model_id: String::new(),
            provider_id: String::new(),
            timeout_ms: agent_common::DEFAULT_TIMEOUT_MS,
            approval_mode: ApprovalMode::default(),
            max_turns: agent_common::MAX_TURNS_PER_SESSION,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ApprovalMode {
    Always,
    Never,
    Dangerous,
}

impl Default for ApprovalMode {
    fn default() -> Self {
        Self::Dangerous
    }
}
