#![deny(clippy::all)]
#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, SystemTime};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelUsageStats {
    pub model_id: String,
    pub total_input_tokens: usize,
    pub total_output_tokens: usize,
    pub total_requests: usize,
    pub total_errors: usize,
    pub total_duration_ms: u64,
    pub cache_hits: usize,
    pub cache_misses: usize,
    pub first_request: Option<SystemTime>,
    pub last_request: Option<SystemTime>,
}

impl ModelUsageStats {
    pub fn new(model_id: impl Into<String>) -> Self {
        Self {
            model_id: model_id.into(),
            total_input_tokens: 0,
            total_output_tokens: 0,
            total_requests: 0,
            total_errors: 0,
            total_duration_ms: 0,
            cache_hits: 0,
            cache_misses: 0,
            first_request: None,
            last_request: None,
        }
    }

    pub fn avg_latency_ms(&self) -> f64 {
        if self.total_requests == 0 {
            0.0
        } else {
            self.total_duration_ms as f64 / self.total_requests as f64
        }
    }

    pub fn error_rate(&self) -> f64 {
        if self.total_requests == 0 {
            0.0
        } else {
            self.total_errors as f64 / self.total_requests as f64
        }
    }

    pub fn cache_hit_rate(&self) -> f64 {
        let total = self.cache_hits + self.cache_misses;
        if total == 0 {
            0.0
        } else {
            self.cache_hits as f64 / total as f64
        }
    }

    pub fn total_tokens(&self) -> usize {
        self.total_input_tokens + self.total_output_tokens
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestRecord {
    pub request_id: String,
    pub model_id: String,
    pub input_tokens: usize,
    pub output_tokens: usize,
    pub duration_ms: u64,
    pub timestamp: SystemTime,
    pub success: bool,
    pub cached: bool,
    pub tool_calls: usize,
}

pub struct ModelContextTracker {
    models: HashMap<String, ModelUsageStats>,
    request_history: Vec<RequestRecord>,
    max_history_size: usize,
    session_start: SystemTime,
    current_model: Option<String>,
}

impl ModelContextTracker {
    pub fn new() -> Self {
        Self {
            models: HashMap::new(),
            request_history: Vec::new(),
            max_history_size: 1000,
            session_start: SystemTime::now(),
            current_model: None,
        }
    }

    pub fn with_max_history(mut self, max: usize) -> Self {
        self.max_history_size = max;
        self
    }

    pub fn set_current_model(&mut self, model_id: impl Into<String>) {
        let model_id = model_id.into();
        self.current_model = Some(model_id.clone());

        self.models
            .entry(model_id.clone())
            .or_insert_with(|| ModelUsageStats::new(&model_id));
    }

    pub fn record_request(
        &mut self,
        request_id: impl Into<String>,
        input_tokens: usize,
        output_tokens: usize,
        duration: Duration,
        success: bool,
        cached: bool,
        tool_calls: usize,
    ) {
        let model_id = match &self.current_model {
            Some(id) => id.clone(),
            None => return,
        };

        let duration_ms = duration.as_millis() as u64;
        let now = SystemTime::now();

        let record = RequestRecord {
            request_id: request_id.into(),
            model_id: model_id.clone(),
            input_tokens,
            output_tokens,
            duration_ms,
            timestamp: now,
            success,
            cached,
            tool_calls,
        };

        self.request_history.push(record);

        if self.request_history.len() > self.max_history_size {
            self.request_history.remove(0);
        }

        if let Some(stats) = self.models.get_mut(&model_id) {
            stats.total_input_tokens += input_tokens;
            stats.total_output_tokens += output_tokens;
            stats.total_requests += 1;
            stats.total_duration_ms += duration_ms;

            if !success {
                stats.total_errors += 1;
            }

            if cached {
                stats.cache_hits += 1;
            } else {
                stats.cache_misses += 1;
            }

            if stats.first_request.is_none() {
                stats.first_request = Some(now);
            }
            stats.last_request = Some(now);
        }
    }

    pub fn get_model_stats(&self, model_id: &str) -> Option<&ModelUsageStats> {
        self.models.get(model_id)
    }

    pub fn get_current_model_stats(&self) -> Option<&ModelUsageStats> {
        self.current_model
            .as_ref()
            .and_then(|id| self.models.get(id))
    }

    pub fn get_all_stats(&self) -> Vec<&ModelUsageStats> {
        self.models.values().collect()
    }

    pub fn session_duration(&self) -> Duration {
        SystemTime::now()
            .duration_since(self.session_start)
            .unwrap_or_default()
    }

    pub fn total_session_tokens(&self) -> usize {
        self.models.values().map(|s| s.total_tokens()).sum()
    }

    pub fn total_session_requests(&self) -> usize {
        self.models.values().map(|s| s.total_requests).sum()
    }

    pub fn recent_requests(&self, count: usize) -> Vec<&RequestRecord> {
        self.request_history.iter().rev().take(count).collect()
    }

    pub fn requests_in_window(&self, duration: Duration) -> Vec<&RequestRecord> {
        let cutoff = SystemTime::now()
            .checked_sub(duration)
            .unwrap_or(self.session_start);

        self.request_history
            .iter()
            .filter(|r| r.timestamp >= cutoff)
            .collect()
    }

    pub fn summary(&self) -> TrackerSummary {
        TrackerSummary {
            total_requests: self.total_session_requests(),
            total_tokens: self.total_session_tokens(),
            models_used: self.models.len(),
            session_duration_secs: self.session_duration().as_secs(),
            current_model: self.current_model.clone(),
        }
    }

    pub fn clear(&mut self) {
        self.models.clear();
        self.request_history.clear();
        self.current_model = None;
    }
}

impl Default for ModelContextTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackerSummary {
    pub total_requests: usize,
    pub total_tokens: usize,
    pub models_used: usize,
    pub session_duration_secs: u64,
    pub current_model: Option<String>,
}
