#![deny(clippy::all)]
#![forbid(unsafe_code)]

use crate::compaction::{CompactionConfig, CompactionResult, CompactionStrategy, Compactor};
use crate::tokenizer::Message;
use crate::tracker::{FileContextTracker, MentionTracker};
use crate::window::ContextWindow;
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct ContextManager {
    window: Arc<RwLock<ContextWindow>>,
    file_tracker: Arc<RwLock<FileContextTracker>>,
    mention_tracker: Arc<RwLock<MentionTracker>>,
    compactor: Compactor,
    config: ContextManagerConfig,
    message_counter: Arc<RwLock<u64>>,
}

#[derive(Debug, Clone)]
pub struct ContextManagerConfig {
    pub max_tokens: usize,
    pub auto_compact: bool,
    pub compact_threshold: f32,
    pub preserve_recent_messages: usize,
    pub max_file_context_tokens: usize,
}

impl Default for ContextManagerConfig {
    fn default() -> Self {
        Self {
            max_tokens: 128_000,
            auto_compact: true,
            compact_threshold: 0.80,
            preserve_recent_messages: 10,
            max_file_context_tokens: 50_000,
        }
    }
}

impl ContextManager {
    pub fn new(config: ContextManagerConfig) -> Self {
        let compaction_config = CompactionConfig {
            strategy: CompactionStrategy::Hybrid,
            target_ratio: 0.6,
            preserve_recent: config.preserve_recent_messages,
            preserve_pinned: true,
            min_messages: 4,
        };

        Self {
            window: Arc::new(RwLock::new(ContextWindow::new(config.max_tokens))),
            file_tracker: Arc::new(RwLock::new(FileContextTracker::new(
                config.max_file_context_tokens,
            ))),
            mention_tracker: Arc::new(RwLock::new(MentionTracker::new())),
            compactor: Compactor::new(compaction_config),
            config,
            message_counter: Arc::new(RwLock::new(0)),
        }
    }

    pub async fn set_system_prompt(&self, prompt: impl Into<String>) {
        let mut window = self.window.write().await;
        window.set_system_prompt(prompt);
    }

    pub async fn add_user_message(&self, content: impl Into<String>) -> String {
        let id = self.next_message_id().await;
        let mut window = self.window.write().await;
        window.add_user(&id, content);
        self.maybe_compact(&mut window).await;
        id
    }

    pub async fn add_assistant_message(&self, content: impl Into<String>) -> String {
        let id = self.next_message_id().await;
        let mut window = self.window.write().await;
        window.add_assistant(&id, content);
        self.maybe_compact(&mut window).await;
        id
    }

    pub async fn add_tool_result(
        &self,
        tool_call_id: impl Into<String>,
        tool_name: impl Into<String>,
        content: impl Into<String>,
    ) -> String {
        let id = self.next_message_id().await;
        let mut window = self.window.write().await;
        window.add_tool_result(&id, tool_call_id, tool_name, content);
        self.maybe_compact(&mut window).await;
        id
    }

    pub async fn add_file_context(&self, path: impl Into<std::path::PathBuf>, content: impl Into<String>) {
        let mut tracker = self.file_tracker.write().await;
        tracker.add_file(path, content);
    }

    pub async fn get_file_context(&self, path: &std::path::PathBuf) -> Option<String> {
        let mut tracker = self.file_tracker.write().await;
        tracker.get_file(path).map(|ctx| ctx.content.clone())
    }

    pub async fn pin_message(&self, id: &str) {
        let mut window = self.window.write().await;
        window.pin_message(id);
    }

    pub async fn force_compact(&self) -> CompactionResult {
        let mut window = self.window.write().await;
        self.compactor.compact(&mut window)
    }

    pub async fn get_messages(&self) -> Vec<Message> {
        let window = self.window.read().await;
        window.to_messages()
    }

    pub async fn get_stats(&self) -> ContextStats {
        let window = self.window.read().await;
        let file_tracker = self.file_tracker.read().await;
        let mention_tracker = self.mention_tracker.read().await;

        ContextStats {
            message_count: window.message_count(),
            message_tokens: window.total_tokens(),
            file_count: file_tracker.file_count(),
            file_tokens: file_tracker.total_tokens(),
            mention_tokens: mention_tracker.total_tokens(),
            total_tokens: window.total_tokens() + file_tracker.total_tokens(),
            max_tokens: self.config.max_tokens,
            available_tokens: window.available_tokens(),
            needs_compaction: window.needs_compaction(),
        }
    }

    pub async fn clear(&self) {
        let mut window = self.window.write().await;
        let mut file_tracker = self.file_tracker.write().await;
        let mut mention_tracker = self.mention_tracker.write().await;

        window.clear();
        file_tracker.clear();
        mention_tracker.clear();
    }

    async fn next_message_id(&self) -> String {
        let mut counter = self.message_counter.write().await;
        *counter += 1;
        format!("msg_{}", *counter)
    }

    async fn maybe_compact(&self, window: &mut ContextWindow) {
        if self.config.auto_compact && window.needs_compaction() {
            self.compactor.compact(window);
        }
    }
}

impl Default for ContextManager {
    fn default() -> Self {
        Self::new(ContextManagerConfig::default())
    }
}

#[derive(Debug, Clone)]
pub struct ContextStats {
    pub message_count: usize,
    pub message_tokens: usize,
    pub file_count: usize,
    pub file_tokens: usize,
    pub mention_tokens: usize,
    pub total_tokens: usize,
    pub max_tokens: usize,
    pub available_tokens: usize,
    pub needs_compaction: bool,
}

impl ContextStats {
    pub fn usage_percent(&self) -> f32 {
        if self.max_tokens == 0 {
            return 0.0;
        }
        (self.total_tokens as f32 / self.max_tokens as f32) * 100.0
    }
}
