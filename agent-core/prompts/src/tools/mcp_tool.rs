#![deny(clippy::all)]
#![forbid(unsafe_code)]

use crate::tools::spec::{task_progress_parameter, ToolId, ToolSpec, ToolSpecParameter};
use crate::types::{ModelFamily, SystemPromptContext};

pub fn mcp_tool_variants() -> Vec<ToolSpec> {
    vec![generic_variant()]
}

fn generic_variant() -> ToolSpec {
    ToolSpec::new(ToolId::McpTool, ModelFamily::Generic)
        .with_name("use_mcp_tool")
        .with_description(
            "Use a tool provided by a connected MCP (Model Context Protocol) server. \
             MCP servers extend capabilities by providing additional tools. \
             Specify the server name, tool name, and arguments as JSON. \
             Available MCP tools and their schemas are listed in the system prompt.",
        )
        .with_context_requirements(has_mcp_servers)
        .with_parameter(server_name_parameter())
        .with_parameter(tool_name_parameter())
        .with_parameter(arguments_parameter())
        .with_parameter(task_progress_parameter())
}

fn has_mcp_servers(context: &SystemPromptContext) -> bool {
    !context.mcp_servers.is_empty()
}

fn server_name_parameter() -> ToolSpecParameter {
    ToolSpecParameter::new(
        "server_name",
        "The name of the MCP server providing the tool. \
         This must match one of the connected servers listed in the system prompt.",
    )
    .with_usage("my-mcp-server")
}

fn tool_name_parameter() -> ToolSpecParameter {
    ToolSpecParameter::new(
        "tool_name",
        "The name of the tool to use from the specified MCP server. \
         Refer to the tool documentation in the system prompt for available tools.",
    )
    .with_usage("fetch_data")
}

fn arguments_parameter() -> ToolSpecParameter {
    ToolSpecParameter::new(
        "arguments",
        "A JSON object containing the arguments for the tool. \
         The schema is defined by the MCP server. Refer to the tool's input schema.",
    )
    .with_usage("{\"query\": \"SELECT * FROM users\"}")
}
