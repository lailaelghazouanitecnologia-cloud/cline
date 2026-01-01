#![deny(clippy::all)]
#![forbid(unsafe_code)]

use crate::message::{ChatMessage, ContentPart, MessageContent, Role, ToolDefinition};
use serde::{Deserialize, Serialize};

pub struct MessageBuilder {
    role: Role,
    parts: Vec<ContentPart>,
}

impl MessageBuilder {
    pub fn user() -> Self {
        Self {
            role: Role::User,
            parts: Vec::new(),
        }
    }

    pub fn assistant() -> Self {
        Self {
            role: Role::Assistant,
            parts: Vec::new(),
        }
    }

    pub fn system() -> Self {
        Self {
            role: Role::System,
            parts: Vec::new(),
        }
    }

    pub fn tool() -> Self {
        Self {
            role: Role::Tool,
            parts: Vec::new(),
        }
    }

    pub fn text(mut self, text: impl Into<String>) -> Self {
        self.parts.push(ContentPart::Text { text: text.into() });
        self
    }

    pub fn tool_use(mut self, id: impl Into<String>, name: impl Into<String>, input: serde_json::Value) -> Self {
        self.parts.push(ContentPart::ToolUse {
            id: id.into(),
            name: name.into(),
            input,
        });
        self
    }

    pub fn tool_result(mut self, tool_use_id: impl Into<String>, content: impl Into<String>) -> Self {
        self.parts.push(ContentPart::ToolResult {
            tool_use_id: tool_use_id.into(),
            content: content.into(),
        });
        self
    }

    pub fn thinking(mut self, thinking: impl Into<String>, signature: Option<String>) -> Self {
        self.parts.push(ContentPart::Thinking {
            thinking: thinking.into(),
            signature,
        });
        self
    }

    pub fn image(mut self, url: impl Into<String>) -> Self {
        self.parts.push(ContentPart::Image {
            image_url: crate::message::ImageUrl {
                url: url.into(),
                detail: None,
            },
        });
        self
    }

    pub fn build(self) -> ChatMessage {
        let content = if self.parts.len() == 1 {
            if let Some(ContentPart::Text { text }) = self.parts.first() {
                MessageContent::Text(text.clone())
            } else {
                MessageContent::Parts(self.parts)
            }
        } else {
            MessageContent::Parts(self.parts)
        };

        ChatMessage {
            role: self.role,
            content,
        }
    }
}

pub struct ConversationBuilder {
    messages: Vec<ChatMessage>,
    system_prompt: Option<String>,
    context_messages: Vec<ChatMessage>,
    max_messages: Option<usize>,
}

impl ConversationBuilder {
    pub fn new() -> Self {
        Self {
            messages: Vec::new(),
            system_prompt: None,
            context_messages: Vec::new(),
            max_messages: None,
        }
    }

    pub fn system(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = Some(prompt.into());
        self
    }

    pub fn add_context(mut self, message: ChatMessage) -> Self {
        self.context_messages.push(message);
        self
    }

    pub fn add(mut self, message: ChatMessage) -> Self {
        self.messages.push(message);
        self
    }

    pub fn user(self, content: impl Into<String>) -> Self {
        self.add(ChatMessage::user(content))
    }

    pub fn assistant(self, content: impl Into<String>) -> Self {
        self.add(ChatMessage::assistant(content))
    }

    pub fn max_messages(mut self, max: usize) -> Self {
        self.max_messages = Some(max);
        self
    }

    pub fn build(mut self) -> Vec<ChatMessage> {
        let mut result = Vec::new();

        if let Some(system) = self.system_prompt {
            result.push(ChatMessage::system(system));
        }

        result.append(&mut self.context_messages);

        if let Some(max) = self.max_messages {
            if self.messages.len() > max {
                let skip = self.messages.len() - max;
                self.messages = self.messages.into_iter().skip(skip).collect();
            }
        }

        result.append(&mut self.messages);
        result
    }
}

impl Default for ConversationBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResultFormatter {
    max_output_length: usize,
    truncation_message: String,
    include_metadata: bool,
}

impl ToolResultFormatter {
    pub fn new() -> Self {
        Self {
            max_output_length: 50000,
            truncation_message: "\n... [output truncated]".to_string(),
            include_metadata: true,
        }
    }

    pub fn with_max_length(mut self, max: usize) -> Self {
        self.max_output_length = max;
        self
    }

    pub fn format_success(&self, tool_name: &str, output: &str) -> String {
        let truncated = self.truncate(output);

        if self.include_metadata {
            format!("[{}] Success:\n{}", tool_name, truncated)
        } else {
            truncated
        }
    }

    pub fn format_error(&self, tool_name: &str, error: &str) -> String {
        if self.include_metadata {
            format!("[{}] Error: {}", tool_name, error)
        } else {
            format!("Error: {}", error)
        }
    }

    pub fn format_file_content(&self, path: &str, content: &str, line_numbers: bool) -> String {
        let truncated = self.truncate(content);

        let formatted = if line_numbers {
            truncated
                .lines()
                .enumerate()
                .map(|(i, line)| format!("{:4} | {}", i + 1, line))
                .collect::<Vec<_>>()
                .join("\n")
        } else {
            truncated
        };

        format!("File: {}\n{}", path, formatted)
    }

    pub fn format_command_output(&self, command: &str, stdout: &str, stderr: &str, exit_code: i32) -> String {
        let mut output = format!("$ {}\n", command);

        if !stdout.is_empty() {
            output.push_str(&self.truncate(stdout));
        }

        if !stderr.is_empty() {
            output.push_str("\n[stderr]\n");
            output.push_str(&self.truncate(stderr));
        }

        output.push_str(&format!("\n[exit code: {}]", exit_code));
        output
    }

    fn truncate(&self, text: &str) -> String {
        if text.len() <= self.max_output_length {
            text.to_string()
        } else {
            let truncated = &text[..self.max_output_length];
            format!("{}{}", truncated, self.truncation_message)
        }
    }
}

impl Default for ToolResultFormatter {
    fn default() -> Self {
        Self::new()
    }
}

pub struct ToolDefinitionBuilder {
    tools: Vec<ToolDefinition>,
}

impl ToolDefinitionBuilder {
    pub fn new() -> Self {
        Self { tools: Vec::new() }
    }

    pub fn add(
        mut self,
        name: impl Into<String>,
        description: impl Into<String>,
        parameters: serde_json::Value,
    ) -> Self {
        self.tools.push(ToolDefinition {
            name: name.into(),
            description: description.into(),
            parameters,
        });
        self
    }

    pub fn add_simple(self, name: impl Into<String>, description: impl Into<String>) -> Self {
        self.add(
            name,
            description,
            serde_json::json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
        )
    }

    pub fn add_with_string_param(
        self,
        name: impl Into<String>,
        description: impl Into<String>,
        param_name: impl Into<String>,
        param_description: impl Into<String>,
        required: bool,
    ) -> Self {
        let param_name = param_name.into();
        let required_vec = if required {
            vec![param_name.clone()]
        } else {
            vec![]
        };

        self.add(
            name,
            description,
            serde_json::json!({
                "type": "object",
                "properties": {
                    param_name: {
                        "type": "string",
                        "description": param_description.into()
                    }
                },
                "required": required_vec
            }),
        )
    }

    pub fn build(self) -> Vec<ToolDefinition> {
        self.tools
    }
}

impl Default for ToolDefinitionBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct ConversationHistory {
    messages: Vec<ChatMessage>,
    max_messages: usize,
    total_tokens: usize,
    token_estimator: fn(&str) -> usize,
}

impl ConversationHistory {
    pub fn new(max_messages: usize) -> Self {
        Self {
            messages: Vec::new(),
            max_messages,
            total_tokens: 0,
            token_estimator: |s| s.len() / 4,
        }
    }

    pub fn with_token_estimator(mut self, estimator: fn(&str) -> usize) -> Self {
        self.token_estimator = estimator;
        self
    }

    pub fn push(&mut self, message: ChatMessage) {
        let tokens = self.estimate_tokens(&message);
        self.total_tokens += tokens;

        self.messages.push(message);

        while self.messages.len() > self.max_messages {
            let removed = self.messages.remove(0);
            let removed_tokens = self.estimate_tokens(&removed);
            self.total_tokens = self.total_tokens.saturating_sub(removed_tokens);
        }
    }

    pub fn push_user(&mut self, content: impl Into<String>) {
        self.push(ChatMessage::user(content));
    }

    pub fn push_assistant(&mut self, content: impl Into<String>) {
        self.push(ChatMessage::assistant(content));
    }

    fn estimate_tokens(&self, message: &ChatMessage) -> usize {
        match &message.content {
            MessageContent::Text(text) => (self.token_estimator)(text),
            MessageContent::Parts(parts) => {
                parts.iter().map(|p| self.estimate_part_tokens(p)).sum()
            }
        }
    }

    fn estimate_part_tokens(&self, part: &ContentPart) -> usize {
        match part {
            ContentPart::Text { text } => (self.token_estimator)(text),
            ContentPart::ToolUse { name, input, .. } => {
                (self.token_estimator)(name) + (self.token_estimator)(&input.to_string())
            }
            ContentPart::ToolResult { content, .. } => (self.token_estimator)(content),
            ContentPart::Thinking { thinking, .. } => (self.token_estimator)(thinking),
            ContentPart::Image { .. } => 1000,
            ContentPart::RedactedThinking { .. } => 0,
        }
    }

    pub fn messages(&self) -> &[ChatMessage] {
        &self.messages
    }

    pub fn len(&self) -> usize {
        self.messages.len()
    }

    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }

    pub fn total_tokens(&self) -> usize {
        self.total_tokens
    }

    pub fn clear(&mut self) {
        self.messages.clear();
        self.total_tokens = 0;
    }

    pub fn last(&self) -> Option<&ChatMessage> {
        self.messages.last()
    }

    pub fn to_vec(&self) -> Vec<ChatMessage> {
        self.messages.clone()
    }
}

impl Default for ConversationHistory {
    fn default() -> Self {
        Self::new(100)
    }
}
