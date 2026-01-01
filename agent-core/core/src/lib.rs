#![deny(clippy::all)]
#![forbid(unsafe_code)]

mod agent;
mod session;
mod state;
mod task_state;

pub use agent::*;
pub use session::*;
pub use state::*;
pub use task_state::*;

pub use agent_common::{AgentError, AgentResult};
pub use agent_config::{Config, ConfigLoader, Feature, Features};
pub use agent_exec::{Executor, Turn, TurnState};
pub use agent_protocol::{
    ApprovalMode, Event, EventMessage, Operation, SessionConfig, SessionId, SessionState,
    Submission,
};
pub use agent_tools::{
    ToolCall, ToolContext, ToolHandler, ToolOutput, ToolRegistry, ToolRouter, ToolSpec,
};
