use agent_common::TurnId;
use serde::{Deserialize, Serialize};

use crate::session::SessionState;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EventMessage {
    SessionStarted(SessionStartedMessage),
    StateChanged(StateChangedMessage),
    AgentThinking(AgentThinkingMessage),
    AgentText(AgentTextMessage),
    AgentTextDelta(AgentTextDeltaMessage),
    ToolCallStarted(ToolCallStartedMessage),
    ToolCallCompleted(ToolCallCompletedMessage),
    ApprovalRequired(ApprovalRequiredMessage),
    TurnCompleted(TurnCompletedMessage),
    Error(ErrorMessage),
    ShutdownComplete,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionStartedMessage {
    pub session_id: String,
    pub model_id: String,
    pub provider_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateChangedMessage {
    pub previous: SessionState,
    pub current: SessionState,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentThinkingMessage {
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTextMessage {
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTextDeltaMessage {
    pub delta: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallStartedMessage {
    pub call_id: String,
    pub tool_name: String,
    pub arguments: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallCompletedMessage {
    pub call_id: String,
    pub tool_name: String,
    pub result: ToolResult,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub success: bool,
    pub output: String,
}

impl ToolResult {
    pub fn success(output: impl Into<String>) -> Self {
        Self {
            success: true,
            output: output.into(),
        }
    }

    pub fn failure(output: impl Into<String>) -> Self {
        Self {
            success: false,
            output: output.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalRequiredMessage {
    pub call_id: String,
    pub tool_name: String,
    pub arguments: serde_json::Value,
    pub risk_level: RiskLevel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RiskLevel {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnCompletedMessage {
    pub turn_id: TurnId,
    pub tool_calls_count: u32,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorMessage {
    pub code: String,
    pub message: String,
    pub recoverable: bool,
}

impl ErrorMessage {
    pub fn new(code: impl Into<String>, message: impl Into<String>, recoverable: bool) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            recoverable,
        }
    }
}
