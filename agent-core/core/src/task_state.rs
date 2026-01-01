#![deny(clippy::all)]
#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::SystemTime;
use tokio::sync::RwLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StreamingState {
    Idle,
    WaitingFirstChunk,
    Streaming,
    ProcessingToolUse,
    Paused,
    Completed,
    Aborted,
    Error,
}

impl Default for StreamingState {
    fn default() -> Self {
        Self::Idle
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AskType {
    FollowUp,
    ToolApproval,
    PlanReview,
    Error,
    Completion,
    Confirmation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingAsk {
    pub ask_type: AskType,
    pub message: String,
    pub tool_name: Option<String>,
    pub options: Vec<String>,
    pub timestamp: SystemTime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolUseEntry {
    pub id: String,
    pub name: String,
    pub input: serde_json::Value,
    pub status: ToolUseStatus,
    pub result: Option<String>,
    pub started_at: SystemTime,
    pub completed_at: Option<SystemTime>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolUseStatus {
    Pending,
    Executing,
    WaitingApproval,
    Approved,
    Rejected,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentBlock {
    pub index: usize,
    pub block_type: ContentBlockType,
    pub content: String,
    pub complete: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContentBlockType {
    Text,
    ToolUse,
    Thinking,
}

#[derive(Debug, Clone, Default)]
pub struct TaskState {
    pub streaming: StreamingState,
    pub abort_requested: bool,
    pub abort_completed: bool,

    pub current_content_index: usize,
    pub content_blocks: Vec<ContentBlock>,
    pub tool_use_map: HashMap<String, ToolUseEntry>,

    pub pending_ask: Option<PendingAsk>,
    pub ask_response: Option<String>,
    pub ask_response_images: Vec<String>,
    pub ask_response_files: Vec<String>,

    pub last_message_ts: Option<SystemTime>,
    pub presentation_locked: bool,
    pub has_pending_updates: bool,

    pub consecutive_mistakes: u32,
    pub did_auto_retry: bool,
    pub did_edit_file: bool,

    pub api_request_count: u32,
    pub requests_since_todo_update: u32,
    pub todo_was_updated_by_user: bool,

    pub checkpoint_error: Option<String>,
    pub currently_summarizing: bool,
    pub last_compact_trigger_index: Option<usize>,

    pub active_hook_execution: Option<String>,
    pub history_deleted_range: Option<(usize, usize)>,
}

impl TaskState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn start_streaming(&mut self) {
        self.streaming = StreamingState::WaitingFirstChunk;
        self.abort_requested = false;
        self.abort_completed = false;
        self.content_blocks.clear();
        self.current_content_index = 0;
    }

    pub fn on_first_chunk(&mut self) {
        self.streaming = StreamingState::Streaming;
        self.last_message_ts = Some(SystemTime::now());
    }

    pub fn on_content_block_start(&mut self, index: usize, block_type: ContentBlockType) {
        self.current_content_index = index;
        self.content_blocks.push(ContentBlock {
            index,
            block_type,
            content: String::new(),
            complete: false,
        });
    }

    pub fn on_content_delta(&mut self, index: usize, delta: &str) {
        if let Some(block) = self.content_blocks.iter_mut().find(|b| b.index == index) {
            block.content.push_str(delta);
            self.has_pending_updates = true;
        }
    }

    pub fn on_content_block_end(&mut self, index: usize) {
        if let Some(block) = self.content_blocks.iter_mut().find(|b| b.index == index) {
            block.complete = true;
        }
    }

    pub fn on_tool_use_start(&mut self, id: &str, name: &str) {
        self.streaming = StreamingState::ProcessingToolUse;
        self.tool_use_map.insert(
            id.to_string(),
            ToolUseEntry {
                id: id.to_string(),
                name: name.to_string(),
                input: serde_json::Value::Null,
                status: ToolUseStatus::Pending,
                result: None,
                started_at: SystemTime::now(),
                completed_at: None,
            },
        );
    }

    pub fn on_tool_use_input(&mut self, id: &str, input: serde_json::Value) {
        if let Some(entry) = self.tool_use_map.get_mut(id) {
            entry.input = input;
        }
    }

    pub fn on_tool_use_executing(&mut self, id: &str) {
        if let Some(entry) = self.tool_use_map.get_mut(id) {
            entry.status = ToolUseStatus::Executing;
        }
    }

    pub fn on_tool_use_complete(&mut self, id: &str, result: &str) {
        if let Some(entry) = self.tool_use_map.get_mut(id) {
            entry.status = ToolUseStatus::Completed;
            entry.result = Some(result.to_string());
            entry.completed_at = Some(SystemTime::now());
        }
        self.streaming = StreamingState::Streaming;
    }

    pub fn on_tool_use_failed(&mut self, id: &str, error: &str) {
        if let Some(entry) = self.tool_use_map.get_mut(id) {
            entry.status = ToolUseStatus::Failed;
            entry.result = Some(error.to_string());
            entry.completed_at = Some(SystemTime::now());
        }
        self.streaming = StreamingState::Streaming;
    }

    pub fn on_stream_complete(&mut self) {
        self.streaming = StreamingState::Completed;
        self.last_message_ts = Some(SystemTime::now());
    }

    pub fn on_stream_error(&mut self, _error: &str) {
        self.streaming = StreamingState::Error;
    }

    pub fn request_abort(&mut self) {
        self.abort_requested = true;
    }

    pub fn complete_abort(&mut self) {
        self.abort_completed = true;
        self.streaming = StreamingState::Aborted;
    }

    pub fn is_streaming(&self) -> bool {
        matches!(
            self.streaming,
            StreamingState::WaitingFirstChunk
                | StreamingState::Streaming
                | StreamingState::ProcessingToolUse
        )
    }

    pub fn is_aborting(&self) -> bool {
        self.abort_requested && !self.abort_completed
    }

    pub fn can_accept_input(&self) -> bool {
        self.pending_ask.is_some() && self.ask_response.is_none()
    }

    pub fn set_ask(&mut self, ask_type: AskType, message: impl Into<String>) {
        self.pending_ask = Some(PendingAsk {
            ask_type,
            message: message.into(),
            tool_name: None,
            options: Vec::new(),
            timestamp: SystemTime::now(),
        });
        self.ask_response = None;
    }

    pub fn set_tool_approval_ask(&mut self, tool_name: &str, message: impl Into<String>) {
        self.pending_ask = Some(PendingAsk {
            ask_type: AskType::ToolApproval,
            message: message.into(),
            tool_name: Some(tool_name.to_string()),
            options: vec!["approve".to_string(), "reject".to_string()],
            timestamp: SystemTime::now(),
        });
        self.ask_response = None;
    }

    pub fn respond_to_ask(&mut self, response: impl Into<String>) {
        self.ask_response = Some(response.into());
    }

    pub fn clear_ask(&mut self) {
        self.pending_ask = None;
        self.ask_response = None;
        self.ask_response_images.clear();
        self.ask_response_files.clear();
    }

    pub fn lock_presentation(&mut self) {
        self.presentation_locked = true;
    }

    pub fn unlock_presentation(&mut self) {
        self.presentation_locked = false;
    }

    pub fn increment_mistakes(&mut self) -> u32 {
        self.consecutive_mistakes += 1;
        self.consecutive_mistakes
    }

    pub fn reset_mistakes(&mut self) {
        self.consecutive_mistakes = 0;
    }

    pub fn record_api_request(&mut self) {
        self.api_request_count += 1;
        self.requests_since_todo_update += 1;
    }

    pub fn reset_todo_request_count(&mut self) {
        self.requests_since_todo_update = 0;
    }

    pub fn start_summarizing(&mut self) {
        self.currently_summarizing = true;
    }

    pub fn finish_summarizing(&mut self, trigger_index: usize) {
        self.currently_summarizing = false;
        self.last_compact_trigger_index = Some(trigger_index);
    }

    pub fn set_hook_execution(&mut self, hook_id: impl Into<String>) {
        self.active_hook_execution = Some(hook_id.into());
    }

    pub fn clear_hook_execution(&mut self) {
        self.active_hook_execution = None;
    }

    pub fn get_current_text(&self) -> String {
        self.content_blocks
            .iter()
            .filter(|b| b.block_type == ContentBlockType::Text)
            .map(|b| b.content.as_str())
            .collect::<Vec<_>>()
            .join("")
    }

    pub fn get_pending_tool_uses(&self) -> Vec<&ToolUseEntry> {
        self.tool_use_map
            .values()
            .filter(|e| e.status == ToolUseStatus::Pending || e.status == ToolUseStatus::WaitingApproval)
            .collect()
    }

    pub fn reset(&mut self) {
        *self = Self::new();
    }
}

pub struct TaskStateManager {
    state: Arc<RwLock<TaskState>>,
}

impl TaskStateManager {
    pub fn new() -> Self {
        Self {
            state: Arc::new(RwLock::new(TaskState::new())),
        }
    }

    pub async fn get(&self) -> TaskState {
        self.state.read().await.clone()
    }

    pub async fn update<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut TaskState) -> R,
    {
        let mut state = self.state.write().await;
        f(&mut state)
    }

    pub async fn start_streaming(&self) {
        self.update(|s| s.start_streaming()).await;
    }

    pub async fn request_abort(&self) {
        self.update(|s| s.request_abort()).await;
    }

    pub async fn complete_abort(&self) {
        self.update(|s| s.complete_abort()).await;
    }

    pub async fn is_streaming(&self) -> bool {
        self.state.read().await.is_streaming()
    }

    pub async fn is_aborting(&self) -> bool {
        self.state.read().await.is_aborting()
    }

    pub async fn set_ask(&self, ask_type: AskType, message: impl Into<String>) {
        let msg = message.into();
        self.update(|s| s.set_ask(ask_type, msg)).await;
    }

    pub async fn respond(&self, response: impl Into<String>) {
        let resp = response.into();
        self.update(|s| s.respond_to_ask(resp)).await;
    }

    pub async fn reset(&self) {
        self.update(|s| s.reset()).await;
    }
}

impl Default for TaskStateManager {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for TaskStateManager {
    fn clone(&self) -> Self {
        Self {
            state: Arc::clone(&self.state),
        }
    }
}
