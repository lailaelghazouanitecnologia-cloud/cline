#![deny(clippy::all)]

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelPricing {
    pub model_id: String,
    pub provider: String,
    pub input_cost_per_1m: f64,
    pub output_cost_per_1m: f64,
    pub cache_read_cost_per_1m: Option<f64>,
    pub cache_write_cost_per_1m: Option<f64>,
}

impl ModelPricing {
    pub fn anthropic_claude_sonnet() -> Self {
        Self {
            model_id: "claude-sonnet-4-20250514".to_string(),
            provider: "anthropic".to_string(),
            input_cost_per_1m: 3.0,
            output_cost_per_1m: 15.0,
            cache_read_cost_per_1m: Some(0.30),
            cache_write_cost_per_1m: Some(3.75),
        }
    }

    pub fn anthropic_claude_opus() -> Self {
        Self {
            model_id: "claude-opus-4-20250514".to_string(),
            provider: "anthropic".to_string(),
            input_cost_per_1m: 15.0,
            output_cost_per_1m: 75.0,
            cache_read_cost_per_1m: Some(1.50),
            cache_write_cost_per_1m: Some(18.75),
        }
    }

    pub fn openai_gpt4o() -> Self {
        Self {
            model_id: "gpt-4o".to_string(),
            provider: "openai".to_string(),
            input_cost_per_1m: 2.50,
            output_cost_per_1m: 10.0,
            cache_read_cost_per_1m: None,
            cache_write_cost_per_1m: None,
        }
    }

    pub fn calculate_cost(&self, usage: &RequestUsage) -> f64 {
        let input_cost = (usage.input_tokens as f64 / 1_000_000.0) * self.input_cost_per_1m;
        let output_cost = (usage.output_tokens as f64 / 1_000_000.0) * self.output_cost_per_1m;

        let cache_cost = match (self.cache_read_cost_per_1m, self.cache_write_cost_per_1m) {
            (Some(read_cost), Some(write_cost)) => {
                let read = (usage.cache_read_tokens as f64 / 1_000_000.0) * read_cost;
                let write = (usage.cache_write_tokens as f64 / 1_000_000.0) * write_cost;
                read + write
            }
            _ => 0.0,
        };

        input_cost + output_cost + cache_cost
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RequestUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub total_tokens: u32,
    pub cache_read_tokens: u32,
    pub cache_write_tokens: u32,
}

impl RequestUsage {
    pub fn new(input: u32, output: u32) -> Self {
        Self {
            input_tokens: input,
            output_tokens: output,
            total_tokens: input + output,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
        }
    }

    pub fn with_cache(mut self, read: u32, write: u32) -> Self {
        self.cache_read_tokens = read;
        self.cache_write_tokens = write;
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageRecord {
    pub timestamp: DateTime<Utc>,
    pub model_id: String,
    pub provider: String,
    pub usage: RequestUsage,
    pub cost_usd: f64,
    pub turn_number: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionUsage {
    pub session_id: String,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_tokens: u64,
    pub total_cache_read: u64,
    pub total_cache_write: u64,
    pub total_cost_usd: f64,
    pub request_count: u32,
    pub records: Vec<UsageRecord>,
}

impl SessionUsage {
    pub fn new(session_id: &str) -> Self {
        Self {
            session_id: session_id.to_string(),
            ..Default::default()
        }
    }

    pub fn add_record(&mut self, record: UsageRecord) {
        self.total_input_tokens += record.usage.input_tokens as u64;
        self.total_output_tokens += record.usage.output_tokens as u64;
        self.total_tokens += record.usage.total_tokens as u64;
        self.total_cache_read += record.usage.cache_read_tokens as u64;
        self.total_cache_write += record.usage.cache_write_tokens as u64;
        self.total_cost_usd += record.cost_usd;
        self.request_count += 1;
        self.records.push(record);
    }

    pub fn to_summary(&self) -> UsageSummary {
        UsageSummary {
            session_id: self.session_id.clone(),
            total_tokens: self.total_tokens,
            total_cost_usd: self.total_cost_usd,
            request_count: self.request_count,
            avg_tokens_per_request: if self.request_count > 0 {
                self.total_tokens / self.request_count as u64
            } else {
                0
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageSummary {
    pub session_id: String,
    pub total_tokens: u64,
    pub total_cost_usd: f64,
    pub request_count: u32,
    pub avg_tokens_per_request: u64,
}

pub struct UsageTracker {
    sessions: Arc<RwLock<HashMap<String, SessionUsage>>>,
    pricing: Arc<RwLock<HashMap<String, ModelPricing>>>,
}

impl UsageTracker {
    pub fn new() -> Self {
        let pricing = Self::default_pricing();
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            pricing: Arc::new(RwLock::new(pricing)),
        }
    }

    fn default_pricing() -> HashMap<String, ModelPricing> {
        let mut map = HashMap::new();
        let sonnet = ModelPricing::anthropic_claude_sonnet();
        let opus = ModelPricing::anthropic_claude_opus();
        let gpt4o = ModelPricing::openai_gpt4o();

        map.insert(sonnet.model_id.clone(), sonnet);
        map.insert(opus.model_id.clone(), opus);
        map.insert(gpt4o.model_id.clone(), gpt4o);
        map
    }

    pub fn record_usage(
        &self,
        session_id: &str,
        model_id: &str,
        provider: &str,
        usage: RequestUsage,
        turn_number: u32,
    ) {
        let cost = self.calculate_cost(model_id, &usage);

        let record = UsageRecord {
            timestamp: Utc::now(),
            model_id: model_id.to_string(),
            provider: provider.to_string(),
            usage,
            cost_usd: cost,
            turn_number,
        };

        let mut sessions = self.sessions.write().unwrap();
        sessions
            .entry(session_id.to_string())
            .or_insert_with(|| SessionUsage::new(session_id))
            .add_record(record);
    }

    fn calculate_cost(&self, model_id: &str, usage: &RequestUsage) -> f64 {
        let pricing = self.pricing.read().unwrap();

        if let Some(model_pricing) = pricing.get(model_id) {
            return model_pricing.calculate_cost(usage);
        }

        for (_, model_pricing) in pricing.iter() {
            if model_id.contains(&model_pricing.model_id) ||
               model_pricing.model_id.contains(model_id) {
                return model_pricing.calculate_cost(usage);
            }
        }

        let default_pricing = ModelPricing::anthropic_claude_sonnet();
        default_pricing.calculate_cost(usage)
    }

    pub fn get_session_usage(&self, session_id: &str) -> Option<SessionUsage> {
        let sessions = self.sessions.read().unwrap();
        sessions.get(session_id).cloned()
    }

    pub fn get_session_summary(&self, session_id: &str) -> Option<UsageSummary> {
        self.get_session_usage(session_id).map(|u| u.to_summary())
    }

    pub fn get_all_summaries(&self) -> Vec<UsageSummary> {
        let sessions = self.sessions.read().unwrap();
        sessions.values().map(|s| s.to_summary()).collect()
    }

    pub fn get_total_cost(&self) -> f64 {
        let sessions = self.sessions.read().unwrap();
        sessions.values().map(|s| s.total_cost_usd).sum()
    }

    pub fn add_pricing(&self, pricing: ModelPricing) {
        let mut map = self.pricing.write().unwrap();
        map.insert(pricing.model_id.clone(), pricing);
    }
}

impl Clone for UsageTracker {
    fn clone(&self) -> Self {
        Self {
            sessions: Arc::clone(&self.sessions),
            pricing: Arc::clone(&self.pricing),
        }
    }
}

impl Default for UsageTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_usage_tracking() {
        let tracker = UsageTracker::new();

        let usage = RequestUsage::new(1000, 500);
        tracker.record_usage("session-1", "claude-sonnet-4-20250514", "anthropic", usage, 1);

        let summary = tracker.get_session_summary("session-1").unwrap();
        assert_eq!(summary.total_tokens, 1500);
        assert_eq!(summary.request_count, 1);
        assert!(summary.total_cost_usd > 0.0);
    }

    #[test]
    fn test_cost_calculation() {
        let pricing = ModelPricing::anthropic_claude_sonnet();
        let usage = RequestUsage::new(1_000_000, 1_000_000);

        let cost = pricing.calculate_cost(&usage);
        assert_eq!(cost, 18.0);
    }

    #[test]
    fn test_multiple_records() {
        let tracker = UsageTracker::new();

        for i in 1..=5 {
            let usage = RequestUsage::new(100, 50);
            tracker.record_usage("session-1", "gpt-4o", "openai", usage, i);
        }

        let summary = tracker.get_session_summary("session-1").unwrap();
        assert_eq!(summary.request_count, 5);
        assert_eq!(summary.total_tokens, 750);
    }
}
