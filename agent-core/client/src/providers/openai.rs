#![deny(clippy::all)]
#![forbid(unsafe_code)]

use crate::message::{
    ChatMessage, ChatRequest, ChatResponse, ContentPart, FinishReason, MessageContent, Role, Usage,
};
use crate::provider::{ChatStreamBox, ModelProvider};
use crate::stream::{DeltaType, StreamDelta, StreamEvent, StreamEventType};
use agent_common::{AgentError, AgentResult};
use async_trait::async_trait;
use futures::stream::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};

pub struct OpenAiProvider {
    client: Client,
    api_key: String,
    model_id: String,
    base_url: String,
}

impl OpenAiProvider {
    pub fn new(api_key: impl Into<String>, model_id: impl Into<String>) -> Self {
        Self {
            client: Client::new(),
            api_key: api_key.into(),
            model_id: model_id.into(),
            base_url: "https://api.openai.com/v1".to_string(),
        }
    }

    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }

    fn build_request(&self, request: &ChatRequest) -> OpenAiRequest {
        let messages: Vec<OpenAiMessage> = request
            .messages
            .iter()
            .map(|m| m.clone().into())
            .collect();

        let tools = request.tools.as_ref().map(|tools| {
            tools
                .iter()
                .map(|t| OpenAiTool {
                    tool_type: "function".to_string(),
                    function: OpenAiFunction {
                        name: t.name.clone(),
                        description: t.description.clone(),
                        parameters: t.parameters.clone(),
                    },
                })
                .collect()
        });

        OpenAiRequest {
            model: self.model_id.clone(),
            messages,
            tools,
            max_tokens: request.max_tokens,
            temperature: request.temperature,
            stop: request.stop.clone(),
            stream: Some(false),
        }
    }

    fn parse_response(&self, response: OpenAiResponse) -> AgentResult<ChatResponse> {
        let choice = response
            .choices
            .into_iter()
            .next()
            .ok_or_else(|| AgentError::provider("no choices in response"))?;

        let content = parse_openai_message(choice.message);
        let finish_reason = parse_finish_reason(&choice.finish_reason);

        Ok(ChatResponse {
            id: response.id,
            content,
            finish_reason,
            usage: Usage {
                prompt_tokens: response.usage.prompt_tokens,
                completion_tokens: response.usage.completion_tokens,
                total_tokens: response.usage.total_tokens,
            },
        })
    }
}

#[async_trait]
impl ModelProvider for OpenAiProvider {
    fn name(&self) -> &str {
        "openai"
    }

    fn model_id(&self) -> &str {
        &self.model_id
    }

    async fn chat(&self, request: ChatRequest) -> AgentResult<ChatResponse> {
        let openai_request = self.build_request(&request);

        let response = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&openai_request)
            .send()
            .await
            .map_err(|e| AgentError::provider(format!("request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(AgentError::provider(format!(
                "API error {}: {}",
                status, body
            )));
        }

        let openai_response: OpenAiResponse = response
            .json()
            .await
            .map_err(|e| AgentError::provider(format!("failed to parse response: {}", e)))?;

        self.parse_response(openai_response)
    }

    async fn chat_stream(&self, request: ChatRequest) -> AgentResult<ChatStreamBox> {
        let mut openai_request = self.build_request(&request);
        openai_request.stream = Some(true);

        let response = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&openai_request)
            .send()
            .await
            .map_err(|e| AgentError::provider(format!("request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(AgentError::provider(format!(
                "API error {}: {}",
                status, body
            )));
        }

        let byte_stream = response.bytes_stream();
        let event_stream = byte_stream
            .map(|result| {
                result
                    .map_err(|e| AgentError::provider(format!("stream error: {}", e)))
                    .and_then(|bytes| parse_sse_chunk(&bytes))
            })
            .filter_map(|result| async move { result.transpose() });

        Ok(Box::pin(event_stream))
    }

    fn supports_vision(&self) -> bool {
        self.model_id.contains("gpt-4") || self.model_id.contains("vision")
    }

    fn max_tokens(&self) -> u32 {
        if self.model_id.contains("gpt-4") {
            16384
        } else {
            4096
        }
    }
}

fn parse_sse_chunk(bytes: &[u8]) -> AgentResult<Option<StreamEvent>> {
    let text = String::from_utf8_lossy(bytes);

    for line in text.lines() {
        if let Some(data) = line.strip_prefix("data: ") {
            if data.trim() == "[DONE]" {
                return Ok(Some(StreamEvent {
                    event_type: StreamEventType::MessageEnd,
                }));
            }

            let chunk: OpenAiStreamChunk = serde_json::from_str(data)
                .map_err(|e| AgentError::provider(format!("failed to parse chunk: {}", e)))?;

            if let Some(choice) = chunk.choices.into_iter().next() {
                if let Some(delta) = choice.delta {
                    if let Some(content) = delta.content {
                        return Ok(Some(StreamEvent {
                            event_type: StreamEventType::ContentBlockDelta {
                                delta: StreamDelta {
                                    delta_type: DeltaType::TextDelta { text: content },
                                    index: choice.index,
                                },
                            },
                        }));
                    }
                }

                if let Some(finish_reason) = choice.finish_reason {
                    return Ok(Some(StreamEvent {
                        event_type: StreamEventType::MessageDelta {
                            finish_reason: parse_finish_reason(&finish_reason),
                            usage: Usage::default(),
                        },
                    }));
                }
            }
        }
    }

    Ok(None)
}

fn parse_openai_message(message: OpenAiMessage) -> MessageContent {
    if let Some(tool_calls) = message.tool_calls {
        let parts: Vec<ContentPart> = tool_calls
            .into_iter()
            .map(|tc| ContentPart::ToolUse {
                id: tc.id,
                name: tc.function.name,
                input: serde_json::from_str(&tc.function.arguments).unwrap_or(serde_json::Value::Null),
            })
            .collect();
        return MessageContent::Parts(parts);
    }

    MessageContent::Text(message.content.unwrap_or_default())
}

fn parse_finish_reason(reason: &str) -> FinishReason {
    match reason {
        "stop" => FinishReason::Stop,
        "length" => FinishReason::Length,
        "tool_calls" => FinishReason::ToolUse,
        "content_filter" => FinishReason::ContentFilter,
        _ => FinishReason::Stop,
    }
}

#[derive(Debug, Serialize)]
struct OpenAiRequest {
    model: String,
    messages: Vec<OpenAiMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<OpenAiTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stop: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
struct OpenAiMessage {
    role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<OpenAiToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct OpenAiToolCall {
    id: String,
    #[serde(rename = "type")]
    call_type: String,
    function: OpenAiToolCallFunction,
}

#[derive(Debug, Serialize, Deserialize)]
struct OpenAiToolCallFunction {
    name: String,
    arguments: String,
}

#[derive(Debug, Serialize)]
struct OpenAiTool {
    #[serde(rename = "type")]
    tool_type: String,
    function: OpenAiFunction,
}

#[derive(Debug, Serialize)]
struct OpenAiFunction {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct OpenAiResponse {
    id: String,
    choices: Vec<OpenAiChoice>,
    usage: OpenAiUsage,
}

#[derive(Debug, Deserialize)]
struct OpenAiChoice {
    message: OpenAiMessage,
    finish_reason: String,
}

#[derive(Debug, Deserialize)]
struct OpenAiUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

#[derive(Debug, Deserialize)]
struct OpenAiStreamChunk {
    choices: Vec<OpenAiStreamChoice>,
}

#[derive(Debug, Deserialize)]
struct OpenAiStreamChoice {
    index: usize,
    delta: Option<OpenAiStreamDelta>,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAiStreamDelta {
    content: Option<String>,
}

impl From<ChatMessage> for OpenAiMessage {
    fn from(message: ChatMessage) -> Self {
        let role = match message.role {
            Role::System => "system",
            Role::User => "user",
            Role::Assistant => "assistant",
            Role::Tool => "tool",
        };

        let content = message.content.as_text().map(|s| s.to_string());

        Self {
            role: role.to_string(),
            content,
            tool_calls: None,
            tool_call_id: None,
        }
    }
}
