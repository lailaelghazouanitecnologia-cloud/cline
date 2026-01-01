use agent_common::{AgentError, AgentResult};
use std::sync::Arc;
use std::time::Instant;

use crate::context::ToolContext;
use crate::registry::ToolRegistry;
use crate::spec::{ToolCall, ToolOutput};

pub struct ToolRouter {
    registry: Arc<ToolRegistry>,
}

impl ToolRouter {
    pub fn new(registry: Arc<ToolRegistry>) -> Self {
        Self { registry }
    }

    pub async fn execute(&self, context: &ToolContext, call: ToolCall) -> AgentResult<ToolOutput> {
        let start = Instant::now();
        let call_id = call.id.clone();
        let tool_name = call.name.clone();

        let handler = self.registry.get(&tool_name).ok_or_else(|| {
            AgentError::not_found(format!("tool: {}", tool_name))
        })?;

        let result = handler.execute(context, call).await;
        let duration_ms = start.elapsed().as_millis() as u64;

        match result {
            Ok(output) => {
                Ok(output.with_call_id(call_id).with_duration(duration_ms))
            }
            Err(error) => Ok(ToolOutput::failure(error.to_string())
                .with_call_id(call_id)
                .with_duration(duration_ms)),
        }
    }

    pub fn is_dangerous(&self, call: &ToolCall) -> bool {
        self.registry
            .get(&call.name)
            .map(|handler| handler.is_dangerous(call))
            .unwrap_or(true)
    }

    pub fn registry(&self) -> &ToolRegistry {
        &self.registry
    }
}
