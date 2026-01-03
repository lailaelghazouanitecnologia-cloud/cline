#![deny(clippy::all)]

use agent_common::AgentResult;
use agent_config::Config;
use agent_core::{LoopConfig, ToolExecutor};
use agent_tools::{ToolCall, ToolContext, ToolRegistry, ToolRouter};
use std::sync::Arc;

pub struct ToolBridge {
    router: Arc<ToolRouter>,
    config: Arc<Config>,
}

impl ToolBridge {
    pub fn new(config: Config) -> Self {
        let mut registry = ToolRegistry::new();
        agent_tools::handlers::register_defaults(&mut registry);

        let router = Arc::new(ToolRouter::new(Arc::new(registry)));
        let config = Arc::new(config);

        Self { router, config }
    }

    pub fn loop_config(&self, max_turns: u32, yolo: bool) -> LoopConfig {
        let auto_approve = if yolo {
            vec![
                "read_file".to_string(),
                "list_files".to_string(),
                "search_files".to_string(),
                "list_code_definitions".to_string(),
                "write_file".to_string(),
                "replace_in_file".to_string(),
                "execute_command".to_string(),
            ]
        } else {
            vec![
                "read_file".to_string(),
                "list_files".to_string(),
                "search_files".to_string(),
            ]
        };

        LoopConfig {
            max_turns,
            turn_timeout: std::time::Duration::from_secs(300),
            total_timeout: std::time::Duration::from_secs(3600),
            auto_approve_tools: auto_approve,
            require_approval: !yolo,
            continue_on_error: false,
        }
    }

    pub async fn execute_tool(&self, name: &str, input: serde_json::Value) -> AgentResult<String> {
        let call = ToolCall {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.to_string(),
            arguments: input,
        };

        let context = ToolContext::new(self.config.clone());
        let output = self.router.execute(&context, call).await?;

        Ok(output.content)
    }
}

#[async_trait::async_trait]
impl ToolExecutor for ToolBridge {
    async fn execute(&self, name: &str, input: serde_json::Value) -> AgentResult<String> {
        self.execute_tool(name, input).await
    }
}
