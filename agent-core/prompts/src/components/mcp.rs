#![deny(clippy::all)]
#![forbid(unsafe_code)]

use crate::types::SystemPromptContext;
use crate::variant::PromptVariant;

pub async fn get_mcp_section(_variant: PromptVariant, context: SystemPromptContext) -> Option<String> {
    if context.mcp_servers.is_empty() {
        return None;
    }

    let mut content = String::from(
        r#"MCP SERVERS

The Model Context Protocol (MCP) enables communication between the system and locally running MCP servers that provide additional tools and resources to extend your capabilities. Each server may provide different capabilities that you can use to accomplish tasks more effectively.

# Connected MCP Servers

The following MCP servers are currently available:"#,
    );

    for server in &context.mcp_servers {
        content.push_str(&format!("\n\n## {}", server));
    }

    if !context.mcp_tools.is_empty() {
        content.push_str("\n\n# Available MCP Tools\n");
        for tool in &context.mcp_tools {
            content.push_str(&format!("\n- {}", tool));
        }
    }

    content.push_str(
        r#"

# MCP Tool Usage Guidelines

1. Each MCP tool call should be made one at a time
2. Wait for confirmation of success before proceeding with additional operations
3. Always handle errors gracefully - if a tool call fails, explain the error and suggest alternatives
4. Use the access_mcp_resource tool to read resources provided by MCP servers
5. Use the use_mcp_tool tool to execute tools provided by MCP servers"#,
    );

    Some(content)
}
