#![deny(clippy::all)]
#![forbid(unsafe_code)]

use crate::types::SystemPromptContext;
use crate::variant::PromptVariant;

pub async fn get_tool_use(_variant: PromptVariant, _context: SystemPromptContext) -> Option<String> {
    Some(TOOL_USE_CONTENT.to_string())
}

const TOOL_USE_CONTENT: &str = r#"TOOL USE GUIDELINES

# Tool Use Formatting

Tool use is formatted using XML-style tags. The tool name is enclosed in opening and closing tags, and each parameter is similarly enclosed within its own set of tags. Here's the structure:

[tool_name]
[parameter1_name]value1[/parameter1_name]
[parameter2_name]value2[/parameter2_name]
...
[/tool_name]

For example:

[read_file]
[path]src/main.rs[/path]
[/read_file]

Always adhere to this format for the tool use to ensure proper parsing and execution.

# Tool Execution Best Practices

1. **One tool at a time**: Execute one tool per response, unless explicitly asked to batch operations
2. **Wait for results**: After using a tool, wait for its results before proceeding
3. **Handle errors gracefully**: If a tool fails, analyze the error and adjust your approach
4. **Validate inputs**: Ensure all required parameters are provided before calling a tool
5. **Use appropriate tools**: Choose the most suitable tool for each subtask

# Common Tool Patterns

## Reading and Understanding Code
1. Use list_files to explore directory structure
2. Use read_file to examine specific files
3. Use search_files to find patterns across the codebase
4. Use list_code_definition_names for code overview

## Making Changes
1. Read the file first to understand current state
2. Use replace_in_file for targeted edits
3. Use write_to_file for new files or complete rewrites
4. Verify changes by reading the file again if needed

## Executing Commands
1. Consider the user's platform and shell
2. Use non-interactive flags when possible
3. Handle long-running processes appropriately
4. Check for existing running terminals before starting servers"#;
