use agent_common::AgentResult;
use std::future::Future;
use std::pin::Pin;

use crate::context::ToolContext;
use crate::spec::{ToolCall, ToolOutput, ToolSpec};

pub type ToolFuture = Pin<Box<dyn Future<Output = AgentResult<ToolOutput>> + Send>>;

pub trait ToolHandler: Send + Sync {
    fn spec(&self) -> ToolSpec;
    fn execute(&self, context: &ToolContext, call: ToolCall) -> ToolFuture;
    fn is_dangerous(&self, call: &ToolCall) -> bool;
}
