use crate::context::ToolContext;
use crate::handler::{ToolFuture, ToolHandler};
use crate::spec::{ToolCall, ToolOutput, ToolSpec};

pub struct ReadFileHandler;

impl ToolHandler for ReadFileHandler {
    fn spec(&self) -> ToolSpec {
        ToolSpec::new("read_file", "Read the contents of a file at the specified path")
            .with_parameter("path", "string", "The path to the file to read", true)
            .with_parameter("offset", "integer", "Line number to start reading from", false)
            .with_parameter("limit", "integer", "Maximum number of lines to read", false)
    }

    fn execute(&self, context: &ToolContext, call: ToolCall) -> ToolFuture {
        let path = call.get_string("path").unwrap_or_default();
        let offset = call.get_u64("offset").unwrap_or(0) as usize;
        let limit = call.get_u64("limit").unwrap_or(2000) as usize;
        let full_path = context.resolve_path(&path);

        Box::pin(async move {
            if !full_path.exists() {
                return Ok(ToolOutput::failure(format!(
                    "file not found: {}",
                    full_path.display()
                )));
            }

            let content = match tokio::fs::read_to_string(&full_path).await {
                Ok(content) => content,
                Err(error) => {
                    return Ok(ToolOutput::failure(error.to_string()));
                }
            };

            let lines: Vec<&str> = content.lines().skip(offset).take(limit).collect();
            let output = lines.join("\n");

            Ok(ToolOutput::success(output))
        })
    }

    fn is_dangerous(&self, _call: &ToolCall) -> bool {
        false
    }
}
