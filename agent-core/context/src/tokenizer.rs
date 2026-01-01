#![deny(clippy::all)]
#![forbid(unsafe_code)]

use tiktoken_rs::{cl100k_base, CoreBPE};
use std::sync::OnceLock;

static TOKENIZER: OnceLock<CoreBPE> = OnceLock::new();

fn get_tokenizer() -> &'static CoreBPE {
    TOKENIZER.get_or_init(|| cl100k_base().expect("failed to load tokenizer"))
}

pub fn count_tokens(text: &str) -> usize {
    get_tokenizer().encode_ordinary(text).len()
}

pub fn count_tokens_messages(messages: &[Message]) -> usize {
    messages.iter().map(|m| count_message_tokens(m)).sum()
}

fn count_message_tokens(message: &Message) -> usize {
    let overhead = 4;
    let role_tokens = count_tokens(&message.role);
    let content_tokens = count_tokens(&message.content);
    overhead + role_tokens + content_tokens
}

pub fn estimate_tokens(char_count: usize) -> usize {
    char_count / 4
}

pub fn truncate_to_tokens(text: &str, max_tokens: usize) -> String {
    let tokenizer = get_tokenizer();
    let tokens = tokenizer.encode_ordinary(text);

    if tokens.len() <= max_tokens {
        return text.to_string();
    }

    let truncated_tokens = &tokens[..max_tokens];
    tokenizer.decode(truncated_tokens.to_vec())
        .unwrap_or_else(|_| text.chars().take(max_tokens * 4).collect())
}

#[derive(Debug, Clone)]
pub struct Message {
    pub role: String,
    pub content: String,
}

impl Message {
    pub fn new(role: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: role.into(),
            content: content.into(),
        }
    }

    pub fn system(content: impl Into<String>) -> Self {
        Self::new("system", content)
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self::new("user", content)
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self::new("assistant", content)
    }

    pub fn token_count(&self) -> usize {
        count_message_tokens(self)
    }
}

pub struct TokenBudget {
    pub max_tokens: usize,
    pub system_reserved: usize,
    pub response_reserved: usize,
}

impl TokenBudget {
    pub fn new(max_tokens: usize) -> Self {
        Self {
            max_tokens,
            system_reserved: max_tokens / 10,
            response_reserved: 4096,
        }
    }

    pub fn available_for_context(&self) -> usize {
        self.max_tokens
            .saturating_sub(self.system_reserved)
            .saturating_sub(self.response_reserved)
    }

    pub fn with_system_reserved(mut self, tokens: usize) -> Self {
        self.system_reserved = tokens;
        self
    }

    pub fn with_response_reserved(mut self, tokens: usize) -> Self {
        self.response_reserved = tokens;
        self
    }
}

impl Default for TokenBudget {
    fn default() -> Self {
        Self::new(128_000)
    }
}
