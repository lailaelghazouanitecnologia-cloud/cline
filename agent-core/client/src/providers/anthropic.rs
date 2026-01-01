#![deny(clippy::all)]
#![forbid(unsafe_code)]

use crate::message::{
    ChatMessage, ChatRequest, ChatResponse, ContentPart, FinishReason, MessageContent, Role,
    Usage,
};
use crate::provider::{ChatStreamBox, ModelProvider};
use crate::stream::{DeltaType, StreamDelta, StreamEvent, StreamEventType};
use agent_common::{AgentError, AgentResult};
use async_trait::async_trait;
use futures::stream::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};

pub struct AnthropicProvider {
    client: Client,
    api_key: String,
    model_id: String,
    base_url: String,
}

impl AnthropicProvider {
    pub fn new(api_key: impl Into<String>, model_id: impl Into<String>) -> Self {
        Self {
            client: Client::new(),
            api_key: api_key.into(),
            model_id: model_id.into(),
            base_url: "https://api.anthropic.com".to_string(),
        }
    }

    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }

    fn build_request(&self, request: &ChatRequest) -> AnthropicRequest {
        let mut system_prompt = None;
        let mut messages = Vec::new();

        for msg in &request.messages {
            match msg.role {
                Role::System => {
                    if let Some(text) = msg.content.as_text() {
                        system_prompt = Some(text.to_string());
                    }
                }
                Role::User | Role::Assistant => {
                    messages.push(AnthropicMessage::from(msg.clone()));
                }
                Role::Tool => {
                    messages.push(AnthropicMessage::from(msg.clone()));
                }
            }
        }

        let tools = request.tools.as_ref().map(|tools| {
            tools
                .iter()
                .map(|t| AnthropicTool {
                    name: t.name.clone(),
                    description: t.description.clone(),
                    input_schema: t.parameters.clone(),
                })
                .collect()
        });

        AnthropicRequest {
            model: self.model_id.clone(),
            messages,
            system: system_prompt,
            tools,
            max_tokens: request.max_tokens.unwrap_or(4096),
            temperature: request.temperature,
            stop_sequences: request.stop.clone(),
            stream: false,
        }
    }

    fn parse_response(&self, response: AnthropicResponse) -> AgentResult<ChatResponse> {
        let content = parse_anthropic_content(response.content);
        let finish_reason = parse_stop_reason(&response.stop_reason.unwrap_or_default());

        Ok(ChatResponse {
            id: response.id,
            content,
            finish_reason,
            usage: Usage {
                prompt_tokens: response.usage.input_tokens,
                completion_tokens: response.usage.output_tokens,
                total_tokens: response.usage.input_tokens + response.usage.output_tokens,
            },
        })
    }
}

#[async_trait]
impl ModelProvider for AnthropicProvider {
    fn name(&self) -> &str {
        "anthropic"
    }

    fn model_id(&self) -> &str {
        &self.model_id
    }

    async fn chat(&self, request: ChatRequest) -> AgentResult<ChatResponse> {
        let anthropic_request = self.build_request(&request);

        let response = self
            .client
            .post(format!("{}/v1/messages", self.base_url))
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("Content-Type", "application/json")
            .json(&anthropic_request)
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

        let anthropic_response: AnthropicResponse = response
            .json()
            .await
            .map_err(|e| AgentError::provider(format!("failed to parse response: {}", e)))?;

        self.parse_response(anthropic_response)
    }

    async fn chat_stream(&self, request: ChatRequest) -> AgentResult<ChatStreamBox> {
        let mut anthropic_request = self.build_request(&request);
        anthropic_request.stream = true;

        let response = self
            .client
            .post(format!("{}/v1/messages", self.base_url))
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("Content-Type", "application/json")
            .json(&anthropic_request)
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
                    .and_then(|bytes| parse_anthropic_sse(&bytes))
            })
            .filter_map(|result| async move { result.transpose() });

        Ok(Box::pin(event_stream))
    }

    fn supports_vision(&self) -> bool {
        true
    }

    fn max_tokens(&self) -> u32 {
        if self.model_id.contains("claude-3-5") {
            8192
        } else {
            4096
        }
    }

    fn context_window(&self) -> u32 {
        200000
    }
}

fn parse_anthropic_sse(bytes: &[u8]) -> AgentResult<Option<StreamEvent>> {
    let text = String::from_utf8_lossy(bytes);

    for line in text.lines() {
        if let Some(data) = line.strip_prefix("data: ") {
            let event: AnthropicStreamEvent = serde_json::from_str(data)
                .map_err(|e| AgentError::provider(format!("failed to parse event: {}", e)))?;

            return Ok(Some(convert_anthropic_event(event)));
        }
    }

    Ok(None)
}

fn convert_anthropic_event(event: AnthropicStreamEvent) -> StreamEvent {
    match event.event_type.as_str() {
        "message_start" => StreamEvent {
            event_type: StreamEventType::MessageStart {
                id: event.message.map(|m| m.id).unwrap_or_default(),
            },
        },
        "content_block_start" => {
            let content_block = event.content_block.map(|cb| match cb.block_type.as_str() {
                "tool_use" => ContentPart::ToolUse {
                    id: cb.id.unwrap_or_default(),
                    name: cb.name.unwrap_or_default(),
                    input: serde_json::Value::Null,
                },
                _ => ContentPart::Text {
                    text: cb.text.unwrap_or_default(),
                },
            });

            StreamEvent {
                event_type: StreamEventType::ContentBlockStart {
                    index: event.index.unwrap_or(0),
                    content_block: content_block.unwrap_or(ContentPart::Text {
                        text: String::new(),
                    }),
                },
            }
        }
        "content_block_delta" => {
            let delta = event.delta.map(|d| match d.delta_type.as_str() {
                "text_delta" => DeltaType::TextDelta {
                    text: d.text.unwrap_or_default(),
                },
                "input_json_delta" => DeltaType::ToolUseInput {
                    input_json: d.partial_json.unwrap_or_default(),
                },
                _ => DeltaType::TextDelta {
                    text: String::new(),
                },
            });

            StreamEvent {
                event_type: StreamEventType::ContentBlockDelta {
                    delta: StreamDelta {
                        delta_type: delta.unwrap_or(DeltaType::TextDelta {
                            text: String::new(),
                        }),
                        index: event.index.unwrap_or(0),
                    },
                },
            }
        }
        "content_block_stop" => StreamEvent {
            event_type: StreamEventType::ContentBlockEnd {
                index: event.index.unwrap_or(0),
            },
        },
        "message_delta" => {
            let stop_reason = event.delta.and_then(|d| d.stop_reason).unwrap_or_default();
            let usage = event.usage.unwrap_or_default();

            StreamEvent {
                event_type: StreamEventType::MessageDelta {
                    finish_reason: parse_stop_reason(&stop_reason),
                    usage: Usage {
                        prompt_tokens: usage.input_tokens,
                        completion_tokens: usage.output_tokens,
                        total_tokens: usage.input_tokens + usage.output_tokens,
                    },
                },
            }
        }
        "message_stop" => StreamEvent {
            event_type: StreamEventType::MessageEnd,
        },
        "error" => StreamEvent {
            event_type: StreamEventType::Error {
                message: event
                    .error
                    .map(|e| e.message)
                    .unwrap_or_else(|| "unknown error".to_string()),
            },
        },
        _ => StreamEvent {
            event_type: StreamEventType::MessageEnd,
        },
    }
}

fn parse_anthropic_content(content: Vec<AnthropicContentBlock>) -> MessageContent {
    if content.len() == 1 {
        if let Some(block) = content.iter().next() {
            if block.block_type == "text" {
                return MessageContent::Text(block.text.clone().unwrap_or_default());
            }
        }
    }

    let parts: Vec<ContentPart> = content
        .into_iter()
        .map(|block| match block.block_type.as_str() {
            "tool_use" => ContentPart::ToolUse {
                id: block.id.unwrap_or_default(),
                name: block.name.unwrap_or_default(),
                input: block.input.unwrap_or(serde_json::Value::Null),
            },
            _ => ContentPart::Text {
                text: block.text.unwrap_or_default(),
            },
        })
        .collect();

    MessageContent::Parts(parts)
}

fn parse_stop_reason(reason: &str) -> FinishReason {
    match reason {
        "end_turn" | "stop_sequence" => FinishReason::Stop,
        "max_tokens" => FinishReason::Length,
        "tool_use" => FinishReason::ToolUse,
        _ => FinishReason::Stop,
    }
}

#[derive(Debug, Serialize)]
struct AnthropicRequest {
    model: String,
    messages: Vec<AnthropicMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<AnthropicTool>>,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stop_sequences: Option<Vec<String>>,
    stream: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct AnthropicMessage {
    role: String,
    content: AnthropicMessageContent,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
enum AnthropicMessageContent {
    Text(String),
    Blocks(Vec<AnthropicContentBlock>),
}

#[derive(Debug, Serialize, Deserialize)]
struct AnthropicContentBlock {
    #[serde(rename = "type")]
    block_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    input: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_use_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
}

#[derive(Debug, Serialize)]
struct AnthropicTool {
    name: String,
    description: String,
    input_schema: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct AnthropicResponse {
    id: String,
    content: Vec<AnthropicContentBlock>,
    stop_reason: Option<String>,
    usage: AnthropicUsage,
}

#[derive(Debug, Deserialize, Default)]
struct AnthropicUsage {
    #[serde(default)]
    input_tokens: u32,
    #[serde(default)]
    output_tokens: u32,
}

#[derive(Debug, Deserialize)]
struct AnthropicStreamEvent {
    #[serde(rename = "type")]
    event_type: String,
    #[serde(default)]
    index: Option<usize>,
    #[serde(default)]
    message: Option<AnthropicStreamMessage>,
    #[serde(default)]
    content_block: Option<AnthropicStreamContentBlock>,
    #[serde(default)]
    delta: Option<AnthropicStreamDelta>,
    #[serde(default)]
    usage: Option<AnthropicUsage>,
    #[serde(default)]
    error: Option<AnthropicError>,
}

#[derive(Debug, Deserialize)]
struct AnthropicStreamMessage {
    id: String,
}

#[derive(Debug, Deserialize)]
struct AnthropicStreamContentBlock {
    #[serde(rename = "type")]
    block_type: String,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    text: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AnthropicStreamDelta {
    #[serde(rename = "type")]
    delta_type: String,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    partial_json: Option<String>,
    #[serde(default)]
    stop_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AnthropicError {
    message: String,
}

impl From<ChatMessage> for AnthropicMessage {
    fn from(message: ChatMessage) -> Self {
        let role = match message.role {
            Role::User | Role::Tool => "user",
            Role::Assistant => "assistant",
            Role::System => "user",
        };

        let content = match message.content {
            MessageContent::Text(text) => AnthropicMessageContent::Text(text),
            MessageContent::Parts(parts) => {
                let blocks: Vec<AnthropicContentBlock> = parts
                    .into_iter()
                    .map(|part| match part {
                        ContentPart::Text { text } => AnthropicContentBlock {
                            block_type: "text".to_string(),
                            text: Some(text),
                            id: None,
                            name: None,
                            input: None,
                            tool_use_id: None,
                            content: None,
                        },
                        ContentPart::ToolUse { id, name, input } => AnthropicContentBlock {
                            block_type: "tool_use".to_string(),
                            text: None,
                            id: Some(id),
                            name: Some(name),
                            input: Some(input),
                            tool_use_id: None,
                            content: None,
                        },
                        ContentPart::ToolResult { tool_use_id, content } => AnthropicContentBlock {
                            block_type: "tool_result".to_string(),
                            text: None,
                            id: None,
                            name: None,
                            input: None,
                            tool_use_id: Some(tool_use_id),
                            content: Some(content),
                        },
                        ContentPart::Image { image_url } => AnthropicContentBlock {
                            block_type: "image".to_string(),
                            text: Some(image_url.url),
                            id: None,
                            name: None,
                            input: None,
                            tool_use_id: None,
                            content: None,
                        },
                    })
                    .collect();
                AnthropicMessageContent::Blocks(blocks)
            }
        };

        Self {
            role: role.to_string(),
            content,
        }
    }
}
