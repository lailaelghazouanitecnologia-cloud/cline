use thiserror::Error;

#[derive(Debug, Error)]
pub enum AgentError {
    #[error("configuration: {message}")]
    Configuration { message: String },

    #[error("io: {source}")]
    Io {
        #[from]
        source: std::io::Error,
    },

    #[error("serialization: {message}")]
    Serialization { message: String },

    #[error("tool failed: {name} - {message}")]
    ToolExecution { name: String, message: String },

    #[error("timeout: {duration_ms}ms")]
    Timeout { duration_ms: u64 },

    #[error("invalid state: expected {expected}, found {found}")]
    InvalidState { expected: String, found: String },

    #[error("channel closed")]
    ChannelClosed,

    #[error("approval required: {tool_name}")]
    ApprovalRequired { tool_name: String },

    #[error("operation rejected")]
    Rejected,

    #[error("limit exceeded: {limit_name} max={max_value}")]
    LimitExceeded { limit_name: String, max_value: u64 },

    #[error("not found: {resource}")]
    NotFound { resource: String },

    #[error("api: {message}")]
    Api { message: String },
}

impl AgentError {
    pub fn configuration(message: impl Into<String>) -> Self {
        Self::Configuration {
            message: message.into(),
        }
    }

    pub fn serialization(message: impl Into<String>) -> Self {
        Self::Serialization {
            message: message.into(),
        }
    }

    pub fn tool_execution(name: impl Into<String>, message: impl Into<String>) -> Self {
        Self::ToolExecution {
            name: name.into(),
            message: message.into(),
        }
    }

    pub fn timeout(duration_ms: u64) -> Self {
        Self::Timeout { duration_ms }
    }

    pub fn invalid_state(expected: impl Into<String>, found: impl Into<String>) -> Self {
        Self::InvalidState {
            expected: expected.into(),
            found: found.into(),
        }
    }

    pub fn approval_required(tool_name: impl Into<String>) -> Self {
        Self::ApprovalRequired {
            tool_name: tool_name.into(),
        }
    }

    pub fn limit_exceeded(limit_name: impl Into<String>, max_value: u64) -> Self {
        Self::LimitExceeded {
            limit_name: limit_name.into(),
            max_value,
        }
    }

    pub fn not_found(resource: impl Into<String>) -> Self {
        Self::NotFound {
            resource: resource.into(),
        }
    }

    pub fn api(message: impl Into<String>) -> Self {
        Self::Api {
            message: message.into(),
        }
    }
}
