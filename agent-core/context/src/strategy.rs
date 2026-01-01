#![deny(clippy::all)]
#![forbid(unsafe_code)]

use crate::window::{ContextMessage, ContextWindow, Importance, MessageRole};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextStrategy {
    Recency,
    Importance,
    Breadth,
    Focused,
    Adaptive,
}

#[derive(Debug, Clone)]
pub struct StrategyConfig {
    pub strategy: ContextStrategy,
    pub max_tokens: usize,
    pub reserve_ratio: f32,
    pub min_messages: usize,
    pub attention_decay: f32,
    pub importance_weights: ImportanceWeights,
}

#[derive(Debug, Clone)]
pub struct ImportanceWeights {
    pub system: f32,
    pub user: f32,
    pub assistant: f32,
    pub tool: f32,
    pub error: f32,
    pub pinned: f32,
}

impl Default for ImportanceWeights {
    fn default() -> Self {
        Self {
            system: 1.0,
            user: 0.8,
            assistant: 0.6,
            tool: 0.5,
            error: 0.9,
            pinned: 1.0,
        }
    }
}

impl Default for StrategyConfig {
    fn default() -> Self {
        Self {
            strategy: ContextStrategy::Adaptive,
            max_tokens: 128_000,
            reserve_ratio: 0.15,
            min_messages: 4,
            attention_decay: 0.95,
            importance_weights: ImportanceWeights::default(),
        }
    }
}

pub struct ContextOptimizer {
    config: StrategyConfig,
    attention_scores: HashMap<String, f32>,
}

impl ContextOptimizer {
    pub fn new(config: StrategyConfig) -> Self {
        Self {
            config,
            attention_scores: HashMap::new(),
        }
    }

    pub fn optimize(&mut self, window: &mut ContextWindow) -> OptimizationResult {
        let target_tokens =
            (self.config.max_tokens as f32 * (1.0 - self.config.reserve_ratio)) as usize;

        if window.total_tokens() <= target_tokens {
            return OptimizationResult::default();
        }

        match self.config.strategy {
            ContextStrategy::Recency => self.optimize_recency(window, target_tokens),
            ContextStrategy::Importance => self.optimize_importance(window, target_tokens),
            ContextStrategy::Breadth => self.optimize_breadth(window, target_tokens),
            ContextStrategy::Focused => self.optimize_focused(window, target_tokens),
            ContextStrategy::Adaptive => self.optimize_adaptive(window, target_tokens),
        }
    }

    fn optimize_recency(
        &mut self,
        window: &mut ContextWindow,
        target_tokens: usize,
    ) -> OptimizationResult {
        let messages = window.messages().to_vec();
        let keep_count = self.config.min_messages.max(
            messages
                .iter()
                .rev()
                .scan(0usize, |acc, m| {
                    *acc += m.token_count;
                    Some(*acc)
                })
                .take_while(|&total| total <= target_tokens)
                .count(),
        );

        let remove_count = messages.len().saturating_sub(keep_count);
        let removed_tokens: usize = messages.iter().take(remove_count).map(|m| m.token_count).sum();

        window.clear();
        for msg in messages.into_iter().skip(remove_count) {
            window.add_message(msg);
        }

        OptimizationResult {
            removed_count: remove_count,
            tokens_freed: removed_tokens,
            strategy_used: ContextStrategy::Recency,
            summary: None,
        }
    }

    fn optimize_importance(
        &mut self,
        window: &mut ContextWindow,
        target_tokens: usize,
    ) -> OptimizationResult {
        let mut messages: Vec<(usize, f32, ContextMessage)> = window
            .messages()
            .iter()
            .enumerate()
            .map(|(i, m)| {
                let score = self.calculate_importance_score(m, i, window.message_count());
                (i, score, m.clone())
            })
            .collect();

        messages.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        let mut kept = Vec::new();
        let mut total_tokens = 0;
        let mut removed_count = 0;
        let mut tokens_freed = 0;

        for (_, _, msg) in messages {
            if total_tokens + msg.token_count <= target_tokens || kept.len() < self.config.min_messages {
                total_tokens += msg.token_count;
                kept.push(msg);
            } else {
                tokens_freed += msg.token_count;
                removed_count += 1;
            }
        }

        kept.sort_by_key(|m| {
            window
                .messages()
                .iter()
                .position(|orig| orig.id == m.id)
                .unwrap_or(0)
        });

        window.clear();
        for msg in kept {
            window.add_message(msg);
        }

        OptimizationResult {
            removed_count,
            tokens_freed,
            strategy_used: ContextStrategy::Importance,
            summary: None,
        }
    }

    fn optimize_breadth(
        &mut self,
        window: &mut ContextWindow,
        target_tokens: usize,
    ) -> OptimizationResult {
        let messages = window.messages().to_vec();
        let total_messages = messages.len();

        if total_messages <= self.config.min_messages {
            return OptimizationResult::default();
        }

        let sample_interval = (total_messages as f32 / self.config.min_messages as f32).ceil() as usize;
        let mut kept = Vec::new();
        let mut total_tokens = 0;

        for (i, msg) in messages.iter().enumerate() {
            let is_boundary = i == 0 || i == total_messages - 1;
            let is_sampled = i % sample_interval == 0;
            let is_important = msg.pinned
                || msg
                    .metadata
                    .as_ref()
                    .map(|m| m.importance == Importance::Critical)
                    .unwrap_or(false);

            if (is_boundary || is_sampled || is_important)
                && total_tokens + msg.token_count <= target_tokens
            {
                total_tokens += msg.token_count;
                kept.push(msg.clone());
            }
        }

        let removed_count = total_messages - kept.len();
        let tokens_freed: usize = messages
            .iter()
            .filter(|m| !kept.iter().any(|k| k.id == m.id))
            .map(|m| m.token_count)
            .sum();

        window.clear();
        for msg in kept {
            window.add_message(msg);
        }

        OptimizationResult {
            removed_count,
            tokens_freed,
            strategy_used: ContextStrategy::Breadth,
            summary: None,
        }
    }

    fn optimize_focused(
        &mut self,
        window: &mut ContextWindow,
        target_tokens: usize,
    ) -> OptimizationResult {
        let messages = window.messages().to_vec();
        let recent_count = (messages.len() / 3).max(self.config.min_messages);
        let early_count = 2;

        let mut kept = Vec::new();
        let mut total_tokens = 0;

        for msg in messages.iter().take(early_count) {
            if total_tokens + msg.token_count <= target_tokens {
                total_tokens += msg.token_count;
                kept.push(msg.clone());
            }
        }

        for msg in messages.iter().rev().take(recent_count) {
            if total_tokens + msg.token_count <= target_tokens {
                total_tokens += msg.token_count;
                kept.push(msg.clone());
            }
        }

        kept.sort_by_key(|m| {
            messages.iter().position(|orig| orig.id == m.id).unwrap_or(0)
        });

        kept.dedup_by(|a, b| a.id == b.id);

        let removed_count = messages.len() - kept.len();
        let tokens_freed: usize = messages
            .iter()
            .filter(|m| !kept.iter().any(|k| k.id == m.id))
            .map(|m| m.token_count)
            .sum();

        window.clear();
        for msg in kept {
            window.add_message(msg);
        }

        OptimizationResult {
            removed_count,
            tokens_freed,
            strategy_used: ContextStrategy::Focused,
            summary: None,
        }
    }

    fn optimize_adaptive(
        &mut self,
        window: &mut ContextWindow,
        target_tokens: usize,
    ) -> OptimizationResult {
        self.update_attention_scores(window);

        let messages = window.messages().to_vec();
        let mut scored: Vec<(f32, ContextMessage)> = messages
            .into_iter()
            .map(|m| {
                let attention = self.attention_scores.get(&m.id).copied().unwrap_or(0.5);
                let recency = 1.0;
                let importance = self.base_importance_score(&m);
                let combined = attention * 0.4 + recency * 0.3 + importance * 0.3;
                (combined, m)
            })
            .collect();

        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

        let mut kept = Vec::new();
        let mut total_tokens = 0;
        let mut removed_count = 0;
        let mut tokens_freed = 0;

        for (_, msg) in scored {
            if total_tokens + msg.token_count <= target_tokens || kept.len() < self.config.min_messages {
                total_tokens += msg.token_count;
                kept.push(msg);
            } else {
                tokens_freed += msg.token_count;
                removed_count += 1;
            }
        }

        kept.sort_by_key(|m| {
            window
                .messages()
                .iter()
                .position(|orig| orig.id == m.id)
                .unwrap_or(0)
        });

        window.clear();
        for msg in kept {
            window.add_message(msg);
        }

        OptimizationResult {
            removed_count,
            tokens_freed,
            strategy_used: ContextStrategy::Adaptive,
            summary: None,
        }
    }

    fn calculate_importance_score(&self, msg: &ContextMessage, index: usize, total: usize) -> f32 {
        let base = self.base_importance_score(msg);
        let recency = (index as f32 / total as f32) * 0.3;
        let pinned_bonus = if msg.pinned { 0.5 } else { 0.0 };

        base + recency + pinned_bonus
    }

    fn base_importance_score(&self, msg: &ContextMessage) -> f32 {
        let role_weight = match msg.role {
            MessageRole::System => self.config.importance_weights.system,
            MessageRole::User => self.config.importance_weights.user,
            MessageRole::Assistant => self.config.importance_weights.assistant,
            MessageRole::Tool => self.config.importance_weights.tool,
        };

        let importance_weight = msg
            .metadata
            .as_ref()
            .map(|m| match m.importance {
                Importance::Critical => 1.0,
                Importance::High => 0.8,
                Importance::Normal => 0.5,
                Importance::Low => 0.2,
            })
            .unwrap_or(0.5);

        role_weight * importance_weight
    }

    fn update_attention_scores(&mut self, window: &ContextWindow) {
        for (_id, score) in self.attention_scores.iter_mut() {
            *score *= self.config.attention_decay;
        }

        let messages = window.messages();
        for (i, msg) in messages.iter().enumerate() {
            let recency_boost = (i as f32 / messages.len() as f32) * 0.5;
            let entry = self.attention_scores.entry(msg.id.clone()).or_insert(0.5);
            *entry = (*entry + recency_boost).min(1.0);
        }
    }

    pub fn boost_attention(&mut self, message_id: &str, amount: f32) {
        let entry = self.attention_scores.entry(message_id.to_string()).or_insert(0.5);
        *entry = (*entry + amount).min(1.0);
    }

    pub fn get_attention(&self, message_id: &str) -> f32 {
        self.attention_scores.get(message_id).copied().unwrap_or(0.5)
    }
}

impl Default for ContextOptimizer {
    fn default() -> Self {
        Self::new(StrategyConfig::default())
    }
}

#[derive(Debug, Clone, Default)]
pub struct OptimizationResult {
    pub removed_count: usize,
    pub tokens_freed: usize,
    pub strategy_used: ContextStrategy,
    pub summary: Option<String>,
}

impl Default for ContextStrategy {
    fn default() -> Self {
        Self::Adaptive
    }
}
