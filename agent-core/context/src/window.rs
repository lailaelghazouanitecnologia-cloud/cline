#![deny(clippy::all)]
#![forbid(unsafe_code)]

use crate::tokenizer::{count_tokens, Message};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextWindow {
    messages: Vec<ContextMessage>,
    system_prompt: Option<String>,
    total_tokens: usize,
    budget: ContextBudget,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextMessage {
    pub id: String,
    pub role: MessageRole,
    pub content: String,
    pub token_count: usize,
    pub pinned: bool,
    pub metadata: Option<MessageMetadata>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageMetadata {
    pub tool_call_id: Option<String>,
    pub tool_name: Option<String>,
    pub file_refs: Vec<String>,
    pub importance: Importance,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Importance {
    Critical,
    High,
    Normal,
    Low,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ContextBudget {
    max_tokens: usize,
    system_reserved: usize,
    response_reserved: usize,
}

impl ContextWindow {
    pub fn new(max_tokens: usize) -> Self {
        Self {
            messages: Vec::new(),
            system_prompt: None,
            total_tokens: 0,
            budget: ContextBudget {
                max_tokens,
                system_reserved: max_tokens / 10,
                response_reserved: 4096,
            },
        }
    }

    pub fn set_system_prompt(&mut self, prompt: impl Into<String>) {
        let prompt = prompt.into();
        let tokens = count_tokens(&prompt);
        self.budget.system_reserved = tokens + 100;
        self.system_prompt = Some(prompt);
    }

    pub fn add_message(&mut self, message: ContextMessage) {
        self.total_tokens += message.token_count;
        self.messages.push(message);
    }

    pub fn add_user(&mut self, id: impl Into<String>, content: impl Into<String>) {
        let content = content.into();
        let token_count = count_tokens(&content);
        self.add_message(ContextMessage {
            id: id.into(),
            role: MessageRole::User,
            content,
            token_count,
            pinned: false,
            metadata: None,
        });
    }

    pub fn add_assistant(&mut self, id: impl Into<String>, content: impl Into<String>) {
        let content = content.into();
        let token_count = count_tokens(&content);
        self.add_message(ContextMessage {
            id: id.into(),
            role: MessageRole::Assistant,
            content,
            token_count,
            pinned: false,
            metadata: None,
        });
    }

    pub fn add_tool_result(
        &mut self,
        id: impl Into<String>,
        tool_call_id: impl Into<String>,
        tool_name: impl Into<String>,
        content: impl Into<String>,
    ) {
        let content = content.into();
        let token_count = count_tokens(&content);
        self.add_message(ContextMessage {
            id: id.into(),
            role: MessageRole::Tool,
            content,
            token_count,
            pinned: false,
            metadata: Some(MessageMetadata {
                tool_call_id: Some(tool_call_id.into()),
                tool_name: Some(tool_name.into()),
                file_refs: Vec::new(),
                importance: Importance::Normal,
            }),
        });
    }

    pub fn pin_message(&mut self, id: &str) {
        if let Some(msg) = self.messages.iter_mut().find(|m| m.id == id) {
            msg.pinned = true;
        }
    }

    pub fn available_tokens(&self) -> usize {
        self.budget
            .max_tokens
            .saturating_sub(self.budget.system_reserved)
            .saturating_sub(self.budget.response_reserved)
            .saturating_sub(self.total_tokens)
    }

    pub fn needs_compaction(&self) -> bool {
        let threshold = self.budget.max_tokens * 80 / 100;
        self.total_tokens + self.budget.system_reserved > threshold
    }

    pub fn total_tokens(&self) -> usize {
        self.total_tokens
    }

    pub fn message_count(&self) -> usize {
        self.messages.len()
    }

    pub fn messages(&self) -> &[ContextMessage] {
        &self.messages
    }

    pub fn system_prompt(&self) -> Option<&str> {
        self.system_prompt.as_deref()
    }

    pub fn to_messages(&self) -> Vec<Message> {
        let mut result = Vec::new();

        if let Some(ref system) = self.system_prompt {
            result.push(Message::system(system.clone()));
        }

        for msg in &self.messages {
            let role = match msg.role {
                MessageRole::System => "system",
                MessageRole::User => "user",
                MessageRole::Assistant => "assistant",
                MessageRole::Tool => "tool",
            };
            result.push(Message::new(role, msg.content.clone()));
        }

        result
    }

    pub fn clear(&mut self) {
        self.messages.clear();
        self.total_tokens = 0;
    }

    pub fn remove_unpinned_before(&mut self, keep_last: usize) {
        if self.messages.len() <= keep_last {
            return;
        }

        let to_check = self.messages.len() - keep_last;
        let mut removed_tokens = 0;
        let mut indices_to_remove = Vec::new();

        for (i, msg) in self.messages.iter().enumerate().take(to_check) {
            if !msg.pinned {
                removed_tokens += msg.token_count;
                indices_to_remove.push(i);
            }
        }

        for i in indices_to_remove.into_iter().rev() {
            self.messages.remove(i);
        }

        self.total_tokens = self.total_tokens.saturating_sub(removed_tokens);
    }
}

impl Default for ContextWindow {
    fn default() -> Self {
        Self::new(128_000)
    }
}
