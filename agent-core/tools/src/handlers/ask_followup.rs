#![deny(clippy::all)]
#![forbid(unsafe_code)]

use crate::context::ToolContext;
use crate::handler::{ToolFuture, ToolHandler};
use crate::spec::{ToolCall, ToolOutput, ToolSpec};

pub struct AskFollowupHandler;

impl AskFollowupHandler {
    pub fn new() -> Self {
        Self
    }
}

impl Default for AskFollowupHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolHandler for AskFollowupHandler {
    fn spec(&self) -> ToolSpec {
        ToolSpec::new(
            "ask_followup_question",
            "Ask the user a question to gather additional information",
        )
        .with_parameter("question", "string", "The question to ask the user", true)
        .with_parameter("options", "array", "Optional list of suggested answers", false)
    }

    fn execute(&self, _context: &ToolContext, call: ToolCall) -> ToolFuture {
        let question = call.get_string("question").unwrap_or_default();
        let options = call
            .get_value("options")
            .and_then(|v| v.as_array().cloned())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect::<Vec<_>>()
            });

        Box::pin(async move {
            let mut output = format!("Question: {}", question);

            if let Some(opts) = options {
                output.push_str("\n\nSuggested options:");
                for (i, opt) in opts.iter().enumerate() {
                    output.push_str(&format!("\n  {}. {}", i + 1, opt));
                }
            }

            Ok(ToolOutput::pending(output))
        })
    }

    fn is_dangerous(&self, _call: &ToolCall) -> bool {
        false
    }
}
