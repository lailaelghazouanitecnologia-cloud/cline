use crate::context::ToolContext;
use crate::handler::{ToolFuture, ToolHandler};
use crate::spec::{ToolCall, ToolOutput, ToolSpec};

pub struct WriteFileHandler;

impl ToolHandler for WriteFileHandler {
    fn spec(&self) -> ToolSpec {
        ToolSpec::new("write_file", "Write content to a file at the specified path")
            .with_parameter("path", "string", "The path to the file to write", true)
            .with_parameter("content", "string", "The content to write to the file", true)
    }

    fn execute(&self, context: &ToolContext, call: ToolCall) -> ToolFuture {
        let path = call.get_string("path").unwrap_or_default();
        let content = call.get_string("content").unwrap_or_default();
        let full_path = context.resolve_path(&path);

        Box::pin(async move {
            if let Some(parent) = full_path.parent() {
                if !parent.exists() {
                    if let Err(error) = tokio::fs::create_dir_all(parent).await {
                        return Ok(ToolOutput::failure(error.to_string()));
                    }
                }
            }

            match tokio::fs::write(&full_path, &content).await {
                Ok(()) => {
                    let message = format!(
                        "wrote {} bytes to {}",
                        content.len(),
                        full_path.display()
                    );
                    Ok(ToolOutput::success(message))
                }
                Err(error) => Ok(ToolOutput::failure(error.to_string())),
            }
        })
    }

    fn is_dangerous(&self, _call: &ToolCall) -> bool {
        true
    }
}
