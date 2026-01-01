use serde_json::json;
use std::time::Instant;

use crate::context::ToolContext;
use crate::handler::{ToolFuture, ToolHandler};
use crate::spec::{ToolCall, ToolOutput, ToolSpec};

pub struct WriteFileHandler;

impl ToolHandler for WriteFileHandler {
    fn spec(&self) -> ToolSpec {
        ToolSpec::new("write_file", "Write content to a file at the specified path")
            .with_parameters(
                json!({
                    "path": {
                        "type": "string",
                        "description": "The path to the file to write"
                    },
                    "content": {
                        "type": "string",
                        "description": "The content to write to the file"
                    }
                }),
                vec![String::from("path"), String::from("content")],
            )
    }

    fn execute(&self, context: &ToolContext, call: ToolCall) -> ToolFuture {
        let path = call.get_string("path").unwrap_or_default();
        let content = call.get_string("content").unwrap_or_default();
        let full_path = context.resolve_path(&path);
        let call_id = call.id.clone();

        Box::pin(async move {
            let start = Instant::now();
            let duration_ms = || start.elapsed().as_millis() as u64;

            if let Some(parent) = full_path.parent() {
                if !parent.exists() {
                    if let Err(error) = tokio::fs::create_dir_all(parent).await {
                        return Ok(ToolOutput::failure(call_id, error.to_string(), duration_ms()));
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
                    Ok(ToolOutput::success(call_id, message, duration_ms()))
                }
                Err(error) => Ok(ToolOutput::failure(call_id, error.to_string(), duration_ms())),
            }
        })
    }

    fn is_dangerous(&self, _call: &ToolCall) -> bool {
        true
    }
}
