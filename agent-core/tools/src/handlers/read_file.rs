use serde_json::json;
use std::time::Instant;

use crate::context::ToolContext;
use crate::handler::{ToolFuture, ToolHandler};
use crate::spec::{ToolCall, ToolOutput, ToolSpec};

pub struct ReadFileHandler;

impl ToolHandler for ReadFileHandler {
    fn spec(&self) -> ToolSpec {
        ToolSpec::new("read_file", "Read the contents of a file at the specified path")
            .with_parameters(
                json!({
                    "path": {
                        "type": "string",
                        "description": "The path to the file to read"
                    },
                    "offset": {
                        "type": "integer",
                        "description": "Line number to start reading from"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of lines to read"
                    }
                }),
                vec![String::from("path")],
            )
    }

    fn execute(&self, context: &ToolContext, call: ToolCall) -> ToolFuture {
        let path = call.get_string("path").unwrap_or_default();
        let offset = call.get_u64("offset").unwrap_or(0) as usize;
        let limit = call.get_u64("limit").unwrap_or(2000) as usize;
        let full_path = context.resolve_path(&path);
        let call_id = call.id.clone();

        Box::pin(async move {
            let start = Instant::now();
            let duration_ms = || start.elapsed().as_millis() as u64;

            if !full_path.exists() {
                return Ok(ToolOutput::failure(
                    call_id,
                    format!("file not found: {}", full_path.display()),
                    duration_ms(),
                ));
            }

            let content = match tokio::fs::read_to_string(&full_path).await {
                Ok(content) => content,
                Err(error) => {
                    return Ok(ToolOutput::failure(call_id, error.to_string(), duration_ms()));
                }
            };

            let lines: Vec<&str> = content.lines().skip(offset).take(limit).collect();
            let output = lines.join("\n");

            Ok(ToolOutput::success(call_id, output, duration_ms()))
        })
    }

    fn is_dangerous(&self, _call: &ToolCall) -> bool {
        false
    }
}
