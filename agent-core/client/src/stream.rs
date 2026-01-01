#![deny(clippy::all)]
#![forbid(unsafe_code)]

use crate::message::{ContentPart, FinishReason, MessageContent, Usage};
use agent_common::AgentResult;
use futures::Stream;
use pin_project_lite::pin_project;
use serde::{Deserialize, Serialize};
use std::pin::Pin;
use std::task::{Context, Poll};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamDelta {
    pub delta_type: DeltaType,
    pub index: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DeltaType {
    TextDelta { text: String },
    ToolUseStart { id: String, name: String },
    ToolUseInput { input_json: String },
    ToolUseEnd,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamEvent {
    pub event_type: StreamEventType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StreamEventType {
    MessageStart { id: String },
    ContentBlockStart { index: usize, content_block: ContentPart },
    ContentBlockDelta { delta: StreamDelta },
    ContentBlockEnd { index: usize },
    MessageDelta { finish_reason: FinishReason, usage: Usage },
    MessageEnd,
    Error { message: String },
}

pin_project! {
    pub struct ChatStream<S> {
        #[pin]
        inner: S,
        buffer: StreamBuffer,
    }
}

#[derive(Debug, Default)]
pub struct StreamBuffer {
    pub id: Option<String>,
    pub content_blocks: Vec<ContentBlockBuffer>,
    pub finish_reason: Option<FinishReason>,
    pub usage: Usage,
}

#[derive(Debug, Clone)]
pub struct ContentBlockBuffer {
    pub content_type: ContentBlockType,
    pub text: String,
    pub tool_id: Option<String>,
    pub tool_name: Option<String>,
    pub tool_input: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentBlockType {
    Text,
    ToolUse,
}

impl<S> ChatStream<S>
where
    S: Stream<Item = AgentResult<StreamEvent>>,
{
    pub fn new(inner: S) -> Self {
        Self {
            inner,
            buffer: StreamBuffer::default(),
        }
    }

    pub fn into_parts(self) -> (S, StreamBuffer) {
        (self.inner, self.buffer)
    }
}

impl<S> Stream for ChatStream<S>
where
    S: Stream<Item = AgentResult<StreamEvent>>,
{
    type Item = AgentResult<StreamEvent>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut this = self.project();

        match this.inner.as_mut().poll_next(cx) {
            Poll::Ready(Some(Ok(event))) => {
                this.buffer.process_event(&event);
                Poll::Ready(Some(Ok(event)))
            }
            Poll::Ready(Some(Err(e))) => Poll::Ready(Some(Err(e))),
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}

impl StreamBuffer {
    pub fn process_event(&mut self, event: &StreamEvent) {
        match &event.event_type {
            StreamEventType::MessageStart { id } => {
                self.id = Some(id.clone());
            }
            StreamEventType::ContentBlockStart { index, content_block } => {
                while self.content_blocks.len() <= *index {
                    self.content_blocks.push(ContentBlockBuffer::new_text());
                }

                match content_block {
                    ContentPart::Text { text } => {
                        self.content_blocks[*index] = ContentBlockBuffer::new_text();
                        self.content_blocks[*index].text = text.clone();
                    }
                    ContentPart::ToolUse { id, name, .. } => {
                        self.content_blocks[*index] = ContentBlockBuffer::new_tool_use();
                        self.content_blocks[*index].tool_id = Some(id.clone());
                        self.content_blocks[*index].tool_name = Some(name.clone());
                    }
                    _ => {}
                }
            }
            StreamEventType::ContentBlockDelta { delta } => {
                if delta.index < self.content_blocks.len() {
                    match &delta.delta_type {
                        DeltaType::TextDelta { text } => {
                            self.content_blocks[delta.index].text.push_str(text);
                        }
                        DeltaType::ToolUseInput { input_json } => {
                            self.content_blocks[delta.index].tool_input.push_str(input_json);
                        }
                        _ => {}
                    }
                }
            }
            StreamEventType::MessageDelta { finish_reason, usage } => {
                self.finish_reason = Some(*finish_reason);
                self.usage = usage.clone();
            }
            _ => {}
        }
    }

    pub fn into_content(mut self) -> MessageContent {
        if self.content_blocks.len() == 1 {
            let block = self.content_blocks.remove(0);
            if block.content_type == ContentBlockType::Text {
                return MessageContent::Text(block.text);
            }
            return MessageContent::Parts(vec![block.into_content_part()]);
        }

        let parts: Vec<ContentPart> = self
            .content_blocks
            .into_iter()
            .map(|block| block.into_content_part())
            .collect();

        MessageContent::Parts(parts)
    }
}

impl ContentBlockBuffer {
    pub fn new_text() -> Self {
        Self {
            content_type: ContentBlockType::Text,
            text: String::new(),
            tool_id: None,
            tool_name: None,
            tool_input: String::new(),
        }
    }

    pub fn new_tool_use() -> Self {
        Self {
            content_type: ContentBlockType::ToolUse,
            text: String::new(),
            tool_id: None,
            tool_name: None,
            tool_input: String::new(),
        }
    }

    pub fn into_content_part(self) -> ContentPart {
        match self.content_type {
            ContentBlockType::Text => ContentPart::Text { text: self.text },
            ContentBlockType::ToolUse => {
                let input = serde_json::from_str(&self.tool_input).unwrap_or(serde_json::Value::Null);
                ContentPart::ToolUse {
                    id: self.tool_id.unwrap_or_default(),
                    name: self.tool_name.unwrap_or_default(),
                    input,
                }
            }
        }
    }
}
