#![deny(clippy::all)]
#![forbid(unsafe_code)]

use agent_common::AgentResult;
use serde::{Deserialize, Serialize};

use crate::{ToolCall, ToolContext, ToolFuture, ToolHandler, ToolOutput, ToolSpec};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResponseType {
    Progress,
    Completion,
    Question,
    ApprovalRequest,
    Error,
    Warning,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActResponse {
    pub response_type: ResponseType,
    pub message: String,
    pub details: Option<String>,
    pub next_steps: Vec<String>,
    pub requires_input: bool,
}

pub struct ActModeRespondHandler;

impl ActModeRespondHandler {
    pub fn new() -> Self {
        Self
    }

    fn parse_input(&self, args: &serde_json::Value) -> AgentResult<ActResponse> {
        let response_type = args
            .get("type")
            .and_then(|v| v.as_str())
            .map(|s| match s {
                "progress" => ResponseType::Progress,
                "completion" => ResponseType::Completion,
                "question" => ResponseType::Question,
                "approval_request" => ResponseType::ApprovalRequest,
                "error" => ResponseType::Error,
                "warning" => ResponseType::Warning,
                _ => ResponseType::Progress,
            })
            .unwrap_or(ResponseType::Progress);

        let message = args
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let details = args
            .get("details")
            .and_then(|v| v.as_str())
            .map(String::from);

        let next_steps = args
            .get("next_steps")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        let requires_input = args
            .get("requires_input")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        Ok(ActResponse {
            response_type,
            message,
            details,
            next_steps,
            requires_input,
        })
    }

    fn format_response(&self, response: &ActResponse) -> String {
        let mut output = String::new();

        let prefix = match response.response_type {
            ResponseType::Progress => "Progress",
            ResponseType::Completion => "Completed",
            ResponseType::Question => "Question",
            ResponseType::ApprovalRequest => "Approval Required",
            ResponseType::Error => "Error",
            ResponseType::Warning => "Warning",
        };

        output.push_str(&format!("## {}\n\n", prefix));
        output.push_str(&response.message);
        output.push_str("\n\n");

        if let Some(details) = &response.details {
            output.push_str(&format!("### Details\n\n{}\n\n", details));
        }

        if !response.next_steps.is_empty() {
            output.push_str("### Next Steps\n\n");
            for (i, step) in response.next_steps.iter().enumerate() {
                output.push_str(&format!("{}. {}\n", i + 1, step));
            }
        }

        output
    }
}

impl Default for ActModeRespondHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolHandler for ActModeRespondHandler {
    fn spec(&self) -> ToolSpec {
        ToolSpec::new(
            "act_mode_respond",
            "Respond to user in Act Mode. Use for progress updates, completions, questions, or requesting approval.",
        )
        .with_parameter("type", "string", "Response type: progress, completion, question, approval_request, error, warning", true)
        .with_parameter("message", "string", "Main message to display", true)
        .with_parameter("details", "string", "Additional details or context", false)
        .with_parameter("next_steps", "array", "Planned next steps", false)
        .with_parameter("requires_input", "boolean", "Whether user input is needed to proceed", false)
    }

    fn execute(&self, _ctx: &ToolContext, call: ToolCall) -> ToolFuture {
        let args = call.to_json_value();
        let handler = Self::new();

        Box::pin(async move {
            let response = handler.parse_input(&args)?;
            let formatted = handler.format_response(&response);

            if response.requires_input {
                Ok(ToolOutput::ask(formatted))
            } else {
                Ok(ToolOutput::success(formatted))
            }
        })
    }

    fn is_dangerous(&self, _call: &ToolCall) -> bool {
        false
    }
}
