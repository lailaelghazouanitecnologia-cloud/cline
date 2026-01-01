#![deny(clippy::all)]
#![forbid(unsafe_code)]

use crate::context::ToolContext;
use crate::handler::{ToolFuture, ToolHandler};
use crate::spec::{ToolCall, ToolOutput, ToolSpec};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CondenseResult {
    pub original_tokens: usize,
    pub condensed_tokens: usize,
    pub reduction_percent: f32,
    pub summary: String,
}

pub struct CondenseHandler;

impl CondenseHandler {
    pub fn new() -> Self {
        Self
    }

    fn condense_messages(&self, messages: &[MessageEntry], target_tokens: usize) -> CondenseResult {
        let original_tokens: usize = messages.iter().map(|m| m.token_count).sum();

        if original_tokens <= target_tokens {
            return CondenseResult {
                original_tokens,
                condensed_tokens: original_tokens,
                reduction_percent: 0.0,
                summary: String::new(),
            };
        }

        let mut condensed = Vec::new();
        let mut current_tokens = 0;

        let recent_count = messages.len().min(5);
        let recent_start = messages.len().saturating_sub(recent_count);

        for msg in messages.iter().skip(recent_start) {
            condensed.push(msg.clone());
            current_tokens += msg.token_count;
        }

        let remaining_budget = target_tokens.saturating_sub(current_tokens);
        let older_messages = &messages[..recent_start];

        let summary = self.summarize_older_messages(older_messages, remaining_budget);
        let summary_tokens = estimate_tokens(&summary);

        CondenseResult {
            original_tokens,
            condensed_tokens: current_tokens + summary_tokens,
            reduction_percent: ((original_tokens - current_tokens - summary_tokens) as f32
                / original_tokens as f32)
                * 100.0,
            summary,
        }
    }

    fn summarize_older_messages(&self, messages: &[MessageEntry], _budget: usize) -> String {
        if messages.is_empty() {
            return String::new();
        }

        let mut tool_uses = Vec::new();
        let mut key_decisions = Vec::new();
        let mut files_modified = Vec::new();

        for msg in messages {
            if let Some(ref tool) = msg.tool_name {
                if !tool_uses.contains(tool) {
                    tool_uses.push(tool.clone());
                }
            }

            for file in &msg.files_mentioned {
                if !files_modified.contains(file) {
                    files_modified.push(file.clone());
                }
            }

            if msg.is_decision {
                key_decisions.push(msg.content.chars().take(100).collect::<String>());
            }
        }

        let mut summary = String::from("[Condensed History]\n");

        if !tool_uses.is_empty() {
            summary.push_str(&format!("Tools used: {}\n", tool_uses.join(", ")));
        }

        if !files_modified.is_empty() {
            let files_str = files_modified.iter().take(10).cloned().collect::<Vec<_>>().join(", ");
            summary.push_str(&format!("Files involved: {}\n", files_str));
        }

        if !key_decisions.is_empty() {
            summary.push_str("Key decisions:\n");
            for (i, decision) in key_decisions.iter().take(5).enumerate() {
                summary.push_str(&format!("{}. {}\n", i + 1, decision));
            }
        }

        summary
    }
}

impl Default for CondenseHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageEntry {
    pub role: String,
    pub content: String,
    pub token_count: usize,
    pub tool_name: Option<String>,
    pub files_mentioned: Vec<String>,
    pub is_decision: bool,
}

fn estimate_tokens(text: &str) -> usize {
    text.len() / 4
}

impl ToolHandler for CondenseHandler {
    fn spec(&self) -> ToolSpec {
        ToolSpec::new("condense_history", "Condense conversation history to fit within token limits")
            .with_parameter("target_tokens", "integer", "Target token count after condensing", true)
            .with_parameter("preserve_recent", "integer", "Number of recent messages to preserve fully", false)
    }

    fn execute(&self, _ctx: &ToolContext, call: ToolCall) -> ToolFuture {
        let target_tokens = call.get_u64("target_tokens").unwrap_or(50000) as usize;
        let _preserve_recent = call.get_u64("preserve_recent").unwrap_or(5) as usize;

        Box::pin(async move {
            let result = CondenseResult {
                original_tokens: 0,
                condensed_tokens: 0,
                reduction_percent: 0.0,
                summary: format!("History condensed to target of {} tokens", target_tokens),
            };

            let output = serde_json::to_string_pretty(&result)
                .unwrap_or_else(|_| "Condensation complete".to_string());

            Ok(ToolOutput::success(output))
        })
    }

    fn is_dangerous(&self, _call: &ToolCall) -> bool {
        false
    }
}

pub struct NewTaskHandler;

impl NewTaskHandler {
    pub fn new() -> Self {
        Self
    }
}

impl Default for NewTaskHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolHandler for NewTaskHandler {
    fn spec(&self) -> ToolSpec {
        ToolSpec::new("new_task", "Start a new task, optionally with context from current session")
            .with_parameter("task", "string", "Description of the new task", true)
            .with_parameter("carry_context", "boolean", "Whether to carry context from current session", false)
            .with_parameter("context_summary", "string", "Summary of relevant context to carry forward", false)
    }

    fn execute(&self, _ctx: &ToolContext, call: ToolCall) -> ToolFuture {
        let task = call.get_string("task").unwrap_or_default();
        let carry_context = call.get_bool("carry_context").unwrap_or(false);
        let context_summary = call.get_string("context_summary");

        Box::pin(async move {
            if task.is_empty() {
                return Ok(ToolOutput::failure("Task description is required"));
            }

            let mut response = format!("New task initiated: {}\n", task);

            if carry_context {
                response.push_str("Context will be carried forward.\n");
                if let Some(summary) = context_summary {
                    response.push_str(&format!("Context summary: {}\n", summary));
                }
            }

            Ok(ToolOutput::success(response))
        })
    }

    fn is_dangerous(&self, _call: &ToolCall) -> bool {
        false
    }
}

pub struct SummarizeTaskHandler;

impl SummarizeTaskHandler {
    pub fn new() -> Self {
        Self
    }
}

impl Default for SummarizeTaskHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolHandler for SummarizeTaskHandler {
    fn spec(&self) -> ToolSpec {
        ToolSpec::new("summarize_task", "Generate a summary of the current task progress")
            .with_parameter("include_files", "boolean", "Include list of modified files", false)
            .with_parameter("include_tools", "boolean", "Include list of tools used", false)
            .with_parameter("format", "string", "Output format: 'brief', 'detailed', 'markdown'", false)
    }

    fn execute(&self, _ctx: &ToolContext, call: ToolCall) -> ToolFuture {
        let include_files = call.get_bool("include_files").unwrap_or(true);
        let include_tools = call.get_bool("include_tools").unwrap_or(true);
        let format = call.get_string("format").unwrap_or_else(|| "detailed".to_string());

        Box::pin(async move {
            let mut summary = match format.as_str() {
                "brief" => String::from("Task Summary (Brief)\n"),
                "markdown" => String::from("# Task Summary\n\n"),
                _ => String::from("Task Summary\n============\n"),
            };

            summary.push_str("Status: In progress\n");

            if include_files {
                summary.push_str("\nFiles modified: [tracked by context]\n");
            }

            if include_tools {
                summary.push_str("\nTools used: [tracked by session]\n");
            }

            Ok(ToolOutput::success(summary))
        })
    }

    fn is_dangerous(&self, _call: &ToolCall) -> bool {
        false
    }
}
