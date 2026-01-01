#![deny(clippy::all)]
#![forbid(unsafe_code)]

use crate::message::{ChatRequest, ChatResponse};
use crate::stream::StreamEvent;
use agent_common::AgentResult;
use async_trait::async_trait;
use futures::Stream;
use std::pin::Pin;

pub type ChatStreamBox = Pin<Box<dyn Stream<Item = AgentResult<StreamEvent>> + Send>>;

#[async_trait]
pub trait ModelProvider: Send + Sync {
    fn name(&self) -> &str;

    fn model_id(&self) -> &str;

    async fn chat(&self, request: ChatRequest) -> AgentResult<ChatResponse>;

    async fn chat_stream(&self, request: ChatRequest) -> AgentResult<ChatStreamBox>;

    fn supports_streaming(&self) -> bool {
        true
    }

    fn supports_tools(&self) -> bool {
        true
    }

    fn supports_vision(&self) -> bool {
        false
    }

    fn max_tokens(&self) -> u32 {
        4096
    }

    fn context_window(&self) -> u32 {
        128000
    }
}

#[derive(Debug, Clone)]
pub struct ProviderInfo {
    pub name: String,
    pub model_id: String,
    pub supports_streaming: bool,
    pub supports_tools: bool,
    pub supports_vision: bool,
    pub max_tokens: u32,
    pub context_window: u32,
}

impl<P: ModelProvider> From<&P> for ProviderInfo {
    fn from(provider: &P) -> Self {
        Self {
            name: provider.name().to_string(),
            model_id: provider.model_id().to_string(),
            supports_streaming: provider.supports_streaming(),
            supports_tools: provider.supports_tools(),
            supports_vision: provider.supports_vision(),
            max_tokens: provider.max_tokens(),
            context_window: provider.context_window(),
        }
    }
}
