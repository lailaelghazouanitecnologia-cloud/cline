use agent_client::providers::OpenAiProvider;
use agent_client::{ChatMessage as AgentMessage, ChatRequest, ModelProvider};
use std::sync::mpsc::Sender;

use crate::StreamMessage;

const PROVIDER_URL: &str = "https://api.groq.com/openai/v1";

pub struct AgentBridge {
    api_key: String,
    model: String,
}

impl AgentBridge {
    pub fn new(api_key: &str, model: &str) -> Self {
        Self {
            api_key: api_key.to_string(),
            model: model.to_string(),
        }
    }

    pub fn call_streaming_sync(
        &self,
        messages: Vec<(String, String)>,
        tx: Sender<StreamMessage>,
    ) {
        let api_key = self.api_key.clone();
        let model = self.model.clone();

        let rt = tokio::runtime::Runtime::new().unwrap();

        rt.block_on(async move {
            let provider = OpenAiProvider::new(&api_key, &model).with_base_url(PROVIDER_URL);

            let chat_messages: Vec<AgentMessage> = messages
                .into_iter()
                .map(|(role, content)| match role.as_str() {
                    "system" => AgentMessage::system(&content),
                    "user" => AgentMessage::user(&content),
                    "assistant" => AgentMessage::assistant(&content),
                    _ => AgentMessage::user(&content),
                })
                .collect();

            let request = ChatRequest {
                messages: chat_messages,
                tools: None,
                max_tokens: Some(8192),
                temperature: Some(0.7),
                stop: None,
            };

            match provider.chat_stream(request).await {
                Ok(stream) => {
                    use agent_client::{ChatStream, DeltaType, StreamEventType};
                    use futures::StreamExt;

                    let mut chat_stream = ChatStream::new(stream);

                    while let Some(event_result) = chat_stream.next().await {
                        match event_result {
                            Ok(event) => {
                                if let StreamEventType::ContentBlockDelta { delta } = &event.event_type {
                                    if let DeltaType::TextDelta { text } = &delta.delta_type {
                                        if tx.send(StreamMessage::Chunk(text.clone())).is_err() {
                                            break;
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                let _ = tx.send(StreamMessage::Error(e.to_string()));
                                return;
                            }
                        }
                    }
                    let _ = tx.send(StreamMessage::Done);
                }
                Err(e) => {
                    let _ = tx.send(StreamMessage::Error(e.to_string()));
                }
            }
        });
    }
}

pub fn get_model() -> String {
    std::env::var("MODEL").unwrap_or_else(|_| "openai/gpt-oss-20b".to_string())
}

pub fn get_system_prompt() -> String {
    "You are a helpful AI coding assistant. You can help with programming tasks, \
     answer questions about code, explain concepts, and assist with debugging. \
     Be concise and clear in your responses. Use markdown for code blocks.".to_string()
}
