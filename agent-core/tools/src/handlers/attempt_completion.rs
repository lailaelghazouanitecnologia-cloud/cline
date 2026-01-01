#![deny(clippy::all)]
#![forbid(unsafe_code)]

use agent_common::AgentResult;
use crate::context::ToolContext;
use crate::handler::{ToolFuture, ToolHandler};
use crate::spec::{ToolCall, ToolOutput, ToolSpec};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot, RwLock};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionAttempt {
    pub result: String,
    pub command: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompletionDecision {
    Accept,
    Reject,
    RequestChanges,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionFeedback {
    pub decision: CompletionDecision,
    pub feedback: Option<String>,
    pub command_output: Option<String>,
}

impl Default for CompletionFeedback {
    fn default() -> Self {
        Self {
            decision: CompletionDecision::Accept,
            feedback: None,
            command_output: None,
        }
    }
}

type CompletionChannel = mpsc::Sender<(String, CompletionAttempt, oneshot::Sender<CompletionFeedback>)>;

pub struct AttemptCompletionHandler {
    channel: Option<CompletionChannel>,
    auto_approve: Arc<RwLock<bool>>,
}

impl AttemptCompletionHandler {
    pub fn new() -> Self {
        Self {
            channel: None,
            auto_approve: Arc::new(RwLock::new(false)),
        }
    }

    pub fn with_channel(channel: CompletionChannel) -> Self {
        Self {
            channel: Some(channel),
            auto_approve: Arc::new(RwLock::new(false)),
        }
    }

    pub async fn set_auto_approve(&self, enabled: bool) {
        *self.auto_approve.write().await = enabled;
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
            "Present the final result to the user after completing all tasks. \
            The user may accept, reject, or request changes. Include a command if \
            there's a way for the user to verify the result.",
        )
        .with_parameter(
            "result",
            "string",
            "A comprehensive summary of what was accomplished, including key changes and outcomes.",
            true,
        )
        .with_parameter(
            "command",
            "string",
            "Optional CLI command the user can run to verify or test the result",
            false,
        )
    }

    fn execute(&self, _context: &ToolContext, call: ToolCall) -> ToolFuture {
        let result = call.get_string("result").unwrap_or_default();
        let command = call.get_string("command");
        let channel = self.channel.clone();
        let auto_approve = self.auto_approve.clone();
        let call_id = call.id.clone();

        Box::pin(async move {
            if result.is_empty() {
                return Ok(ToolOutput::failure("Missing required parameter: result"));
            }

            if *auto_approve.read().await {
                return Ok(ToolOutput::completion(format!(
                    "[AUTO-APPROVED] Task completed:\n\n{}",
                    result
                )));
            }

            let attempt = CompletionAttempt {
                result: result.clone(),
                command: command.clone(),
            };

            match channel {
                Some(sender) => {
                    handle_channel_completion(sender, call_id, attempt, result, command).await
                }
                None => build_pending_output(result, command),
            }
        })
    }

    fn is_dangerous(&self, _call: &ToolCall) -> bool {
        false
    }
}

async fn handle_channel_completion(
    sender: CompletionChannel,
    call_id: String,
    attempt: CompletionAttempt,
    result: String,
    command: Option<String>,
) -> AgentResult<ToolOutput> {
    let (response_tx, response_rx) = oneshot::channel();

    if sender.send((call_id, attempt, response_tx)).await.is_err() {
        return Ok(ToolOutput::failure("Failed to send completion to user interface"));
    }

    match response_rx.await {
        Ok(feedback) => build_feedback_output(feedback, result),
        Err(_) => Ok(ToolOutput::failure("User did not respond to completion attempt")),
    }
}

fn build_feedback_output(
    feedback: CompletionFeedback,
    result: String,
) -> AgentResult<ToolOutput> {
    match feedback.decision {
        CompletionDecision::Accept => {
            let mut output = format!("<completion_accepted>\n{}\n</completion_accepted>", result);

            if let Some(cmd_output) = feedback.command_output {
                output.push_str(&format!("\n<command_output>\n{}\n</command_output>", cmd_output));
            }

            Ok(ToolOutput::completion(output))
        }
        CompletionDecision::Reject => {
            let reason = feedback.feedback.unwrap_or_else(|| "No reason provided".to_string());
            Ok(ToolOutput::failure(format!(
                "<completion_rejected>\nThe user rejected the completion.\nReason: {}\n\
                You should address the user's concerns and try again.\n</completion_rejected>",
                reason
            )))
        }
        CompletionDecision::RequestChanges => {
            let changes = feedback.feedback.unwrap_or_else(|| "No details provided".to_string());
            Ok(ToolOutput::success(format!(
                "<changes_requested>\n{}\n</changes_requested>\n\n\
                The user has requested changes. Please address these and attempt completion again.",
                changes
            )))
        }
    }
}

fn build_pending_output(
    result: String,
    command: Option<String>,
) -> AgentResult<ToolOutput> {
    let mut output = format!("[Awaiting user review]\n\nResult:\n{}", result);

    if let Some(cmd) = command {
        output.push_str(&format!("\n\nVerification command:\n```\n{}\n```", cmd));
    }

    Ok(ToolOutput::pending(output))
}

pub fn create_completion_channel() -> (
    CompletionChannel,
    mpsc::Receiver<(String, CompletionAttempt, oneshot::Sender<CompletionFeedback>)>,
) {
    mpsc::channel(10)
}
