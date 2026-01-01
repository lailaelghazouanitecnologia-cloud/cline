#![deny(clippy::all)]
#![forbid(unsafe_code)]

use crate::handler::{ToolFuture, ToolHandler};
use crate::spec::ToolSpec;
use crate::{ToolCall, ToolContext, ToolOutput};
use agent_common::AgentError;
use agent_mcp::{McpServerManager, ToolCallResponse, ToolContent};
use std::sync::Arc;

pub struct UseMcpToolHandler {
    manager: Arc<McpServerManager>,
}

impl UseMcpToolHandler {
    pub fn new(manager: Arc<McpServerManager>) -> Self {
        Self { manager }
    }

    fn format_response(&self, response: &ToolCallResponse) -> String {
        let mut output = String::new();

        for content in &response.content {
            match content {
                ToolContent::Text { text } => {
                    output.push_str(text);
                    output.push('\n');
                }
                ToolContent::Image { mime_type, data } => {
                    output.push_str(&format!(
                        "<image mime_type=\"{}\" data_length=\"{}\" />\n",
                        mime_type,
                        data.len()
                    ));
                }
                ToolContent::Resource { resource } => {
                    if let Some(ref text) = resource.text {
                        output.push_str(text);
                        output.push('\n');
                    } else {
                        output.push_str(&format!("<resource uri=\"{}\" />\n", resource.uri));
                    }
                }
            }
        }

        output.trim().to_string()
    }
}

impl ToolHandler for UseMcpToolHandler {
    fn spec(&self) -> ToolSpec {
        ToolSpec::new("use_mcp_tool", "Execute a tool provided by an MCP server")
            .with_parameter("server_name", "string", "Name of the MCP server", true)
            .with_parameter("tool_name", "string", "Name of the tool to execute", true)
            .with_parameter("arguments", "object", "Arguments to pass to the tool", true)
    }

    fn execute(&self, _ctx: &ToolContext, call: ToolCall) -> ToolFuture {
        let manager = Arc::clone(&self.manager);
        let args = call.to_json_value();

        Box::pin(async move {
            let tool_name = args
                .get("tool_name")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AgentError::validation("tool_name is required"))?;

            let arguments = args
                .get("arguments")
                .cloned()
                .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));

            let response = manager.call_tool(tool_name, arguments).await?;

            let handler = UseMcpToolHandler { manager };
            let output = handler.format_response(&response);

            if response.is_error {
                Ok(ToolOutput::failure(output))
            } else {
                Ok(ToolOutput::success(output))
            }
        })
    }

    fn is_dangerous(&self, _call: &ToolCall) -> bool {
        true
    }
}

pub struct AccessMcpResourceHandler {
    manager: Arc<McpServerManager>,
}

impl AccessMcpResourceHandler {
    pub fn new(manager: Arc<McpServerManager>) -> Self {
        Self { manager }
    }
}

impl ToolHandler for AccessMcpResourceHandler {
    fn spec(&self) -> ToolSpec {
        ToolSpec::new("access_mcp_resource", "Read a resource from an MCP server")
            .with_parameter("server_name", "string", "Name of the MCP server", true)
            .with_parameter("uri", "string", "URI of the resource to read", true)
    }

    fn execute(&self, _ctx: &ToolContext, call: ToolCall) -> ToolFuture {
        let manager = Arc::clone(&self.manager);
        let args = call.to_json_value();

        Box::pin(async move {
            let server_name = args
                .get("server_name")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AgentError::validation("server_name is required"))?;

            let uri = args
                .get("uri")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AgentError::validation("uri is required"))?;

            let content = manager.read_resource(server_name, uri).await?;

            Ok(ToolOutput::success(content))
        })
    }

    fn is_dangerous(&self, _call: &ToolCall) -> bool {
        false
    }
}

pub struct ListMcpToolsHandler {
    manager: Arc<McpServerManager>,
}

impl ListMcpToolsHandler {
    pub fn new(manager: Arc<McpServerManager>) -> Self {
        Self { manager }
    }
}

impl ToolHandler for ListMcpToolsHandler {
    fn spec(&self) -> ToolSpec {
        ToolSpec::new("list_mcp_tools", "List all available MCP tools")
            .with_parameter("server_name", "string", "Filter by server name", false)
    }

    fn execute(&self, _ctx: &ToolContext, call: ToolCall) -> ToolFuture {
        let manager = Arc::clone(&self.manager);
        let args = call.to_json_value();

        Box::pin(async move {
            let server_filter = args.get("server_name").and_then(|v| v.as_str());

            let tools = manager.list_all_tools().await;

            let mut output = String::new();

            for (server, tool) in tools {
                if let Some(filter) = server_filter {
                    if server != filter {
                        continue;
                    }
                }

                output.push_str(&format!(
                    "- {} (server: {})\n  {}\n",
                    tool.name, server, tool.description
                ));
            }

            if output.is_empty() {
                output = "No MCP tools available.".to_string();
            }

            Ok(ToolOutput::success(output))
        })
    }

    fn is_dangerous(&self, _call: &ToolCall) -> bool {
        false
    }
}
