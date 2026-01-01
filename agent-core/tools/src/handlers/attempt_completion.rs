#![deny(clippy::all)]
#![forbid(unsafe_code)]

use crate::context::ToolContext;
use crate::handler::{ToolFuture, ToolHandler};
use crate::spec::{ToolCall, ToolOutput, ToolSpec};

pub struct AttemptCompletionHandler;

impl AttemptCompletionHandler {
    pub fn new() -> Self {
        Self
    }
}

impl Default for AttemptCompletionHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolHandler for AttemptCompletionHandler {
    fn spec(&self) -> ToolSpec {
        ToolSpec::new(
            "attempt_completion",
            "Signal that the task has been completed",
        )
        .with_parameter("result", "string", "Summary of what was accomplished", true)
        .with_parameter(
            "command",
            "string",
            "Optional command for user to run to verify",
            false,
        )
    }

    fn execute(&self, _context: &ToolContext, call: ToolCall) -> ToolFuture {
        let result = call.get_string("result").unwrap_or_default();
        let command = call.get_string("command");

        Box::pin(async move {
            let mut output = format!("Task completed:\n\n{}", result);

            if let Some(cmd) = command {
                output.push_str(&format!(
                    "\n\nTo verify, you can run:\n```\n{}\n```",
                    cmd
                ));
            }

            Ok(ToolOutput::completion(output))
        })
    }

    fn is_dangerous(&self, _call: &ToolCall) -> bool {
        false
    }
}
