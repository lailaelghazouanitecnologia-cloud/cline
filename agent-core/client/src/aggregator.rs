#![deny(clippy::all)]
#![forbid(unsafe_code)]

use crate::stream::{StreamBuffer, StreamEvent, StreamEventType};
use agent_common::AgentResult;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamProgress {
    pub message_id: Option<String>,
    pub content_blocks_started: usize,
    pub content_blocks_completed: usize,
    pub text_chars_received: usize,
    pub tool_uses_started: usize,
    pub tool_uses_completed: usize,
    pub thinking_blocks: usize,
    pub is_complete: bool,
    pub has_error: bool,
    pub error_message: Option<String>,
}

impl Default for StreamProgress {
    fn default() -> Self {
        Self {
            message_id: None,
            content_blocks_started: 0,
            content_blocks_completed: 0,
            text_chars_received: 0,
            tool_uses_started: 0,
            tool_uses_completed: 0,
            thinking_blocks: 0,
            is_complete: false,
            has_error: false,
            error_message: None,
        }
    }
}

pub struct StreamAggregator {
    buffer: StreamBuffer,
    progress: StreamProgress,
    event_queue: VecDeque<StreamEvent>,
    max_queue_size: usize,
    subscribers: Vec<mpsc::Sender<StreamProgress>>,
}

impl StreamAggregator {
    pub fn new() -> Self {
        Self {
            buffer: StreamBuffer::default(),
            progress: StreamProgress::default(),
            event_queue: VecDeque::new(),
            max_queue_size: 1000,
            subscribers: Vec::new(),
        }
    }

    pub fn with_queue_size(mut self, size: usize) -> Self {
        self.max_queue_size = size;
        self
    }

    pub fn subscribe_progress(&mut self) -> mpsc::Receiver<StreamProgress> {
        let (tx, rx) = mpsc::channel(100);
        self.subscribers.push(tx);
        rx
    }

    pub async fn process_event(&mut self, event: StreamEvent) -> AgentResult<()> {
        self.update_progress(&event);
        self.buffer.process_event(&event);

        if self.event_queue.len() >= self.max_queue_size {
            self.event_queue.pop_front();
        }
        self.event_queue.push_back(event);

        self.notify_subscribers().await;
        Ok(())
    }

    fn update_progress(&mut self, event: &StreamEvent) {
        match &event.event_type {
            StreamEventType::MessageStart { id } => {
                self.progress.message_id = Some(id.clone());
            }
            StreamEventType::ContentBlockStart { content_block, .. } => {
                self.progress.content_blocks_started += 1;
                match content_block {
                    crate::message::ContentPart::ToolUse { .. } => {
                        self.progress.tool_uses_started += 1;
                    }
                    crate::message::ContentPart::Thinking { .. } => {
                        self.progress.thinking_blocks += 1;
                    }
                    _ => {}
                }
            }
            StreamEventType::ContentBlockDelta { delta } => {
                if let crate::stream::DeltaType::TextDelta { text } = &delta.delta_type {
                    self.progress.text_chars_received += text.len();
                }
            }
            StreamEventType::ContentBlockEnd { .. } => {
                self.progress.content_blocks_completed += 1;
            }
            StreamEventType::MessageEnd => {
                self.progress.is_complete = true;
            }
            StreamEventType::Error { message } => {
                self.progress.has_error = true;
                self.progress.error_message = Some(message.clone());
            }
            _ => {}
        }
    }

    async fn notify_subscribers(&self) {
        let progress = self.progress.clone();
        for tx in &self.subscribers {
            let _ = tx.send(progress.clone()).await;
        }
    }

    pub fn get_progress(&self) -> &StreamProgress {
        &self.progress
    }

    pub fn get_buffer(&self) -> &StreamBuffer {
        &self.buffer
    }

    pub fn take_buffer(self) -> StreamBuffer {
        self.buffer
    }

    pub fn get_events(&self) -> &VecDeque<StreamEvent> {
        &self.event_queue
    }

    pub fn clear_events(&mut self) {
        self.event_queue.clear();
    }

    pub fn reset(&mut self) {
        self.buffer = StreamBuffer::default();
        self.progress = StreamProgress::default();
        self.event_queue.clear();
    }
}

impl Default for StreamAggregator {
    fn default() -> Self {
        Self::new()
    }
}

pub struct SharedStreamAggregator {
    inner: Arc<RwLock<StreamAggregator>>,
}

impl SharedStreamAggregator {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(StreamAggregator::new())),
        }
    }

    pub async fn process_event(&self, event: StreamEvent) -> AgentResult<()> {
        let mut agg = self.inner.write().await;
        agg.process_event(event).await
    }

    pub async fn get_progress(&self) -> StreamProgress {
        let agg = self.inner.read().await;
        agg.progress.clone()
    }

    pub async fn subscribe_progress(&self) -> mpsc::Receiver<StreamProgress> {
        let mut agg = self.inner.write().await;
        agg.subscribe_progress()
    }

    pub async fn is_complete(&self) -> bool {
        let agg = self.inner.read().await;
        agg.progress.is_complete
    }

    pub async fn has_error(&self) -> bool {
        let agg = self.inner.read().await;
        agg.progress.has_error
    }

    pub async fn get_text_so_far(&self) -> String {
        let agg = self.inner.read().await;
        let mut text = String::new();
        for block in &agg.buffer.content_blocks {
            if block.content_type == crate::stream::ContentBlockType::Text {
                text.push_str(&block.text);
            }
        }
        text
    }

    pub async fn get_tool_uses(&self) -> Vec<ToolUseProgress> {
        let agg = self.inner.read().await;
        agg.buffer
            .content_blocks
            .iter()
            .filter(|b| b.content_type == crate::stream::ContentBlockType::ToolUse)
            .map(|b| ToolUseProgress {
                id: b.tool_id.clone().unwrap_or_default(),
                name: b.tool_name.clone().unwrap_or_default(),
                input_json: b.tool_input.clone(),
            })
            .collect()
    }

    pub async fn reset(&self) {
        let mut agg = self.inner.write().await;
        agg.reset();
    }

    pub fn clone_inner(&self) -> Arc<RwLock<StreamAggregator>> {
        self.inner.clone()
    }
}

impl Default for SharedStreamAggregator {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for SharedStreamAggregator {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolUseProgress {
    pub id: String,
    pub name: String,
    pub input_json: String,
}

pub struct BackpressureController {
    high_watermark: usize,
    low_watermark: usize,
    current_buffer_size: Arc<RwLock<usize>>,
    pause_sender: Option<mpsc::Sender<bool>>,
}

impl BackpressureController {
    pub fn new(low: usize, high: usize) -> Self {
        Self {
            high_watermark: high,
            low_watermark: low,
            current_buffer_size: Arc::new(RwLock::new(0)),
            pause_sender: None,
        }
    }

    pub fn subscribe_pause(&mut self) -> mpsc::Receiver<bool> {
        let (tx, rx) = mpsc::channel(10);
        self.pause_sender = Some(tx);
        rx
    }

    pub async fn add_bytes(&self, bytes: usize) {
        let mut size = self.current_buffer_size.write().await;
        *size += bytes;

        if *size >= self.high_watermark {
            if let Some(ref tx) = self.pause_sender {
                let _ = tx.send(true).await;
            }
        }
    }

    pub async fn consume_bytes(&self, bytes: usize) {
        let mut size = self.current_buffer_size.write().await;
        *size = size.saturating_sub(bytes);

        if *size <= self.low_watermark {
            if let Some(ref tx) = self.pause_sender {
                let _ = tx.send(false).await;
            }
        }
    }

    pub async fn current_size(&self) -> usize {
        *self.current_buffer_size.read().await
    }

    pub async fn should_pause(&self) -> bool {
        *self.current_buffer_size.read().await >= self.high_watermark
    }
}

pub struct StreamCollector {
    text_chunks: Vec<String>,
    tool_inputs: Vec<(String, String, String)>,
    thinking_chunks: Vec<String>,
    complete: bool,
}

impl StreamCollector {
    pub fn new() -> Self {
        Self {
            text_chunks: Vec::new(),
            tool_inputs: Vec::new(),
            thinking_chunks: Vec::new(),
            complete: false,
        }
    }

    pub fn add_text(&mut self, text: String) {
        self.text_chunks.push(text);
    }

    pub fn add_tool_use(&mut self, id: String, name: String, input: String) {
        self.tool_inputs.push((id, name, input));
    }

    pub fn add_thinking(&mut self, thinking: String) {
        self.thinking_chunks.push(thinking);
    }

    pub fn mark_complete(&mut self) {
        self.complete = true;
    }

    pub fn is_complete(&self) -> bool {
        self.complete
    }

    pub fn full_text(&self) -> String {
        self.text_chunks.join("")
    }

    pub fn full_thinking(&self) -> String {
        self.thinking_chunks.join("")
    }

    pub fn tool_uses(&self) -> &[(String, String, String)] {
        &self.tool_inputs
    }

    pub fn text_chunk_count(&self) -> usize {
        self.text_chunks.len()
    }
}

impl Default for StreamCollector {
    fn default() -> Self {
        Self::new()
    }
}
