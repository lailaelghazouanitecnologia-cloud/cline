#![deny(clippy::all)]
#![forbid(unsafe_code)]

use crate::window::{ContextMessage, ContextWindow, Importance, MessageRole};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompactionStrategy {
    Summarize,
    TruncateOld,
    RemoveLowImportance,
    Sliding,
    Hybrid,
}

#[derive(Debug, Clone)]
pub struct CompactionConfig {
    pub strategy: CompactionStrategy,
    pub target_ratio: f32,
    pub preserve_recent: usize,
    pub preserve_pinned: bool,
    pub min_messages: usize,
}

impl Default for CompactionConfig {
    fn default() -> Self {
        Self {
            strategy: CompactionStrategy::Hybrid,
            target_ratio: 0.6,
            preserve_recent: 10,
            preserve_pinned: true,
            min_messages: 4,
        }
    }
}

pub struct CompactionResult {
    pub removed_count: usize,
    pub tokens_freed: usize,
    pub summary: Option<String>,
    pub preserved_messages: Vec<String>,
}

pub struct Compactor {
    config: CompactionConfig,
}

impl Compactor {
    pub fn new(config: CompactionConfig) -> Self {
        Self { config }
    }

    pub fn compact(&self, window: &mut ContextWindow) -> CompactionResult {
        match self.config.strategy {
            CompactionStrategy::TruncateOld => self.truncate_old(window),
            CompactionStrategy::RemoveLowImportance => self.remove_low_importance(window),
            CompactionStrategy::Sliding => self.sliding_window(window),
            CompactionStrategy::Hybrid => self.hybrid_compact(window),
            CompactionStrategy::Summarize => self.prepare_for_summary(window),
        }
    }

    fn truncate_old(&self, window: &mut ContextWindow) -> CompactionResult {
        let target_tokens = (window.total_tokens() as f32 * self.config.target_ratio) as usize;
        let mut removed_count = 0;
        let mut tokens_freed = 0;
        let preserved = Vec::new();

        let messages = window.messages().to_vec();
        let preserve_start = messages.len().saturating_sub(self.config.preserve_recent);

        window.clear();

        for (i, msg) in messages.into_iter().enumerate() {
            let should_preserve = i >= preserve_start
                || (self.config.preserve_pinned && msg.pinned)
                || window.total_tokens() + msg.token_count <= target_tokens;

            if should_preserve {
                window.add_message(msg);
            } else {
                removed_count += 1;
                tokens_freed += msg.token_count;
            }
        }

        CompactionResult {
            removed_count,
            tokens_freed,
            summary: None,
            preserved_messages: preserved,
        }
    }

    fn remove_low_importance(&self, window: &mut ContextWindow) -> CompactionResult {
        let mut removed_count = 0;
        let mut tokens_freed = 0;

        let messages = window.messages().to_vec();
        let preserve_start = messages.len().saturating_sub(self.config.preserve_recent);

        window.clear();

        for (i, msg) in messages.into_iter().enumerate() {
            let importance = msg
                .metadata
                .as_ref()
                .map(|m| m.importance)
                .unwrap_or(Importance::Normal);

            let should_keep = i >= preserve_start
                || (self.config.preserve_pinned && msg.pinned)
                || importance == Importance::Critical
                || importance == Importance::High;

            if should_keep {
                window.add_message(msg);
            } else {
                removed_count += 1;
                tokens_freed += msg.token_count;
            }
        }

        CompactionResult {
            removed_count,
            tokens_freed,
            summary: None,
            preserved_messages: Vec::new(),
        }
    }

    fn sliding_window(&self, window: &mut ContextWindow) -> CompactionResult {
        let messages = window.messages().to_vec();
        let keep_count = self.config.preserve_recent.max(self.config.min_messages);

        if messages.len() <= keep_count {
            return CompactionResult {
                removed_count: 0,
                tokens_freed: 0,
                summary: None,
                preserved_messages: Vec::new(),
            };
        }

        let mut tokens_freed = 0;
        let removed_count = messages.len() - keep_count;

        let mut pinned_to_preserve = Vec::new();
        for msg in messages.iter().take(removed_count) {
            if self.config.preserve_pinned && msg.pinned {
                pinned_to_preserve.push(msg.clone());
            } else {
                tokens_freed += msg.token_count;
            }
        }

        window.clear();

        for msg in pinned_to_preserve {
            window.add_message(msg);
        }

        for msg in messages.into_iter().skip(removed_count) {
            window.add_message(msg);
        }

        CompactionResult {
            removed_count,
            tokens_freed,
            summary: None,
            preserved_messages: Vec::new(),
        }
    }

    fn hybrid_compact(&self, window: &mut ContextWindow) -> CompactionResult {
        let initial_tokens = window.total_tokens();
        let target = (initial_tokens as f32 * self.config.target_ratio) as usize;

        let result1 = self.remove_low_importance(window);

        if window.total_tokens() <= target {
            return result1;
        }

        let result2 = self.truncate_old(window);

        CompactionResult {
            removed_count: result1.removed_count + result2.removed_count,
            tokens_freed: result1.tokens_freed + result2.tokens_freed,
            summary: None,
            preserved_messages: Vec::new(),
        }
    }

    fn prepare_for_summary(&self, window: &mut ContextWindow) -> CompactionResult {
        let messages = window.messages().to_vec();
        let preserve_start = messages.len().saturating_sub(self.config.preserve_recent);

        let to_summarize: Vec<_> = messages
            .iter()
            .take(preserve_start)
            .filter(|m| !(self.config.preserve_pinned && m.pinned))
            .collect();

        let summary_content = self.generate_summary_prompt(&to_summarize);

        let mut tokens_freed = 0;
        let removed_count = to_summarize.len();

        window.clear();

        for (i, msg) in messages.into_iter().enumerate() {
            if i >= preserve_start || (self.config.preserve_pinned && msg.pinned) {
                window.add_message(msg);
            } else {
                tokens_freed += msg.token_count;
            }
        }

        CompactionResult {
            removed_count,
            tokens_freed,
            summary: Some(summary_content),
            preserved_messages: Vec::new(),
        }
    }

    fn generate_summary_prompt(&self, messages: &[&ContextMessage]) -> String {
        let mut content = String::from("Previous conversation summary:\n");

        for msg in messages {
            let role = match msg.role {
                MessageRole::User => "User",
                MessageRole::Assistant => "Assistant",
                MessageRole::Tool => "Tool",
                MessageRole::System => "System",
            };

            let preview: String = msg.content.chars().take(200).collect();
            content.push_str(&format!("- {}: {}...\n", role, preview));
        }

        content
    }
}

impl Default for Compactor {
    fn default() -> Self {
        Self::new(CompactionConfig::default())
    }
}
