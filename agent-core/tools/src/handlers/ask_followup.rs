#![deny(clippy::all)]
#![forbid(unsafe_code)]

use crate::context::ToolContext;
use crate::handler::{ToolFuture, ToolHandler};
use crate::spec::{ToolCall, ToolOutput, ToolSpec};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot, RwLock};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FollowupQuestion {
    pub question: String,
    pub options: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FollowupAnswer {
    pub text: String,
    pub selected_option: Option<String>,
    pub images: Vec<String>,
    pub files: Vec<String>,
}

impl Default for FollowupAnswer {
    fn default() -> Self {
        Self {
            text: String::new(),
            selected_option: None,
            images: Vec::new(),
            files: Vec::new(),
        }
    }
}

type QuestionChannel = mpsc::Sender<(String, FollowupQuestion, oneshot::Sender<FollowupAnswer>)>;

pub struct AskFollowupHandler {
    channel: Option<QuestionChannel>,
    yolo_mode: Arc<RwLock<bool>>,
}

impl AskFollowupHandler {
    pub fn new() -> Self {
        Self {
            channel: None,
            yolo_mode: Arc::new(RwLock::new(false)),
        }
    }

    pub fn with_channel(channel: QuestionChannel) -> Self {
        Self {
            channel: Some(channel),
            yolo_mode: Arc::new(RwLock::new(false)),
        }
    }

    pub async fn set_yolo_mode(&self, enabled: bool) {
        *self.yolo_mode.write().await = enabled;
    }

    fn parse_options(options_value: Option<&serde_json::Value>) -> Vec<String> {
        match options_value {
            Some(serde_json::Value::Array(arr)) => arr
                .iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect(),
            Some(serde_json::Value::String(s)) => {
                serde_json::from_str(s).unwrap_or_default()
            }
            _ => Vec::new(),
        }
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
            "Ask the user a question to gather additional information needed to complete the task. \
            Use this when you need clarification, additional details, or user preferences. \
            Provide clear, specific questions and optionally suggest answer options.",
        )
        .with_parameter(
            "question",
            "string",
            "The question to ask the user. Be specific and clear about what information you need.",
            true,
        )
        .with_parameter(
            "options",
            "array",
            "Optional list of suggested answer options for the user to choose from",
            false,
        )
    }

    fn execute(&self, _context: &ToolContext, call: ToolCall) -> ToolFuture {
        let question = call.get_string("question").unwrap_or_default();
        let options = Self::parse_options(call.get_value("options").as_ref());
        let channel = self.channel.clone();
        let yolo_mode = self.yolo_mode.clone();
        let call_id = call.id.clone();

        Box::pin(async move {
            if question.is_empty() {
                return Ok(ToolOutput::failure("Missing required parameter: question"));
            }

            if *yolo_mode.read().await {
                let truncated = if question.len() > 100 {
                    format!("{}...", &question[..100])
                } else {
                    question.clone()
                };

                return Ok(ToolOutput::success(format!(
                    "[YOLO MODE: User input is not available in non-interactive mode. \
                    You must use available tools (read_file, list_files, search_files, etc.) \
                    to gather the information you need instead of asking the user. \
                    Proceed with using tools to find the answer to your question: \"{}\"]",
                    truncated
                )));
            }

            let followup = FollowupQuestion {
                question: question.clone(),
                options: options.clone(),
            };

            match channel {
                Some(sender) => {
                    let (response_tx, response_rx) = oneshot::channel();

                    if sender.send((call_id, followup, response_tx)).await.is_err() {
                        return Ok(ToolOutput::failure(
                            "Failed to send question to user interface",
                        ));
                    }

                    match response_rx.await {
                        Ok(answer) => {
                            let mut result = format!("<answer>\n{}\n</answer>", answer.text);

                            if let Some(ref selected) = answer.selected_option {
                                result = format!(
                                    "<selected_option>{}</selected_option>\n{}",
                                    selected, result
                                );
                            }

                            if !answer.files.is_empty() {
                                result.push_str("\n<attached_files>\n");
                                for file in &answer.files {
                                    result.push_str(&format!("- {}\n", file));
                                }
                                result.push_str("</attached_files>");
                            }

                            Ok(ToolOutput::success(result))
                        }
                        Err(_) => Ok(ToolOutput::failure(
                            "User did not respond to the question",
                        )),
                    }
                }
                None => {
                    let mut output = format!("[Waiting for user response]\n\nQuestion: {}", question);

                    if !options.is_empty() {
                        output.push_str("\n\nSuggested options:");
                        for (i, opt) in options.iter().enumerate() {
                            output.push_str(&format!("\n  {}. {}", i + 1, opt));
                        }
                    }

                    Ok(ToolOutput::pending(output))
                }
            }
        })
    }

    fn is_dangerous(&self, _call: &ToolCall) -> bool {
        false
    }
}

pub fn create_followup_channel() -> (
    QuestionChannel,
    mpsc::Receiver<(String, FollowupQuestion, oneshot::Sender<FollowupAnswer>)>,
) {
    mpsc::channel(10)
}
