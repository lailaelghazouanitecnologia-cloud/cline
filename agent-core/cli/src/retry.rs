#![deny(clippy::all)]

use agent_common::AgentError;
use std::future::Future;
use std::time::Duration;
use tokio::time::sleep;

#[derive(Clone)]
pub struct RetryConfig {
    pub max_retries: u32,
    pub base_delay_ms: u64,
    pub max_delay_ms: u64,
    pub retry_all_errors: bool,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            base_delay_ms: 1000,
            max_delay_ms: 10000,
            retry_all_errors: false,
        }
    }
}

impl RetryConfig {
    pub fn with_max_retries(mut self, n: u32) -> Self {
        self.max_retries = n;
        self
    }

    pub fn with_base_delay(mut self, ms: u64) -> Self {
        self.base_delay_ms = ms;
        self
    }

    pub fn with_max_delay(mut self, ms: u64) -> Self {
        self.max_delay_ms = ms;
        self
    }

    pub fn retry_all(mut self) -> Self {
        self.retry_all_errors = true;
        self
    }
}

#[derive(Debug, Clone)]
pub enum RetryableError {
    RateLimit { retry_after_ms: Option<u64> },
    Timeout,
    NetworkError { message: String },
    ServiceUnavailable,
}

pub fn is_retryable(error: &AgentError) -> Option<RetryableError> {
    match error {
        AgentError::Timeout { .. } => Some(RetryableError::Timeout),
        AgentError::Api { message } => {
            if message.contains("429") || message.to_lowercase().contains("rate limit") {
                Some(RetryableError::RateLimit { retry_after_ms: None })
            } else if message.contains("503") || message.to_lowercase().contains("unavailable") {
                Some(RetryableError::ServiceUnavailable)
            } else if message.to_lowercase().contains("timeout") {
                Some(RetryableError::Timeout)
            } else if message.to_lowercase().contains("connection") {
                Some(RetryableError::NetworkError {
                    message: message.clone(),
                })
            } else {
                None
            }
        }
        AgentError::Provider { message } => {
            if message.contains("429") || message.to_lowercase().contains("rate limit") {
                Some(RetryableError::RateLimit { retry_after_ms: None })
            } else {
                None
            }
        }
        AgentError::Io { .. } => Some(RetryableError::NetworkError {
            message: "IO error".to_string(),
        }),
        _ => None,
    }
}

pub struct RetryState {
    pub attempt: u32,
    pub max_attempts: u32,
    pub delay_ms: u64,
    pub error_type: RetryableError,
}

impl RetryState {
    pub fn format_message(&self) -> String {
        let error_desc = match &self.error_type {
            RetryableError::RateLimit { retry_after_ms } => {
                if let Some(ms) = retry_after_ms {
                    format!("Rate limited (retry after {}s)", ms / 1000)
                } else {
                    "Rate limited".to_string()
                }
            }
            RetryableError::Timeout => "Request timeout".to_string(),
            RetryableError::NetworkError { message } => format!("Network error: {}", message),
            RetryableError::ServiceUnavailable => "Service unavailable".to_string(),
        };

        format!(
            "{}. Retry {}/{} in {:.1}s",
            error_desc,
            self.attempt,
            self.max_attempts,
            self.delay_ms as f64 / 1000.0
        )
    }
}

pub type OnRetryFn = Box<dyn Fn(&RetryState) + Send + Sync>;

pub async fn with_retry<T, F, Fut>(
    config: &RetryConfig,
    on_retry: Option<&OnRetryFn>,
    operation: F,
) -> Result<T, AgentError>
where
    F: Fn() -> Fut,
    Fut: Future<Output = Result<T, AgentError>>,
{
    let mut last_error: Option<AgentError> = None;

    for attempt in 0..config.max_retries {
        match operation().await {
            Ok(result) => return Ok(result),
            Err(error) => {
                let retryable = is_retryable(&error);
                let is_last = attempt == config.max_retries - 1;

                if !config.retry_all_errors && retryable.is_none() {
                    return Err(error);
                }

                if is_last {
                    return Err(error);
                }

                let delay_ms = calculate_delay(config, attempt, &retryable);

                if let Some(callback) = on_retry {
                    let state = RetryState {
                        attempt: attempt + 1,
                        max_attempts: config.max_retries,
                        delay_ms,
                        error_type: retryable.unwrap_or(RetryableError::NetworkError {
                            message: error.to_string(),
                        }),
                    };
                    callback(&state);
                }

                sleep(Duration::from_millis(delay_ms)).await;
                last_error = Some(error);
            }
        }
    }

    Err(last_error.unwrap_or_else(|| AgentError::internal("Retry loop exhausted")))
}

fn calculate_delay(config: &RetryConfig, attempt: u32, retryable: &Option<RetryableError>) -> u64 {
    if let Some(RetryableError::RateLimit {
        retry_after_ms: Some(ms),
    }) = retryable
    {
        return *ms;
    }

    let exponential = config.base_delay_ms * 2u64.pow(attempt);
    exponential.min(config.max_delay_ms)
}

pub struct RetryDisplay;

impl RetryDisplay {
    pub fn show_retry(state: &RetryState) {
        eprintln!(
            "\x1b[33m⟳\x1b[0m {}",
            state.format_message()
        );
    }

    pub fn show_retry_success(attempt: u32) {
        eprintln!(
            "\x1b[32m✓\x1b[0m Request succeeded after {} attempt{}",
            attempt,
            if attempt > 1 { "s" } else { "" }
        );
    }

    pub fn show_retry_exhausted(max_attempts: u32) {
        eprintln!(
            "\x1b[31m✗\x1b[0m Request failed after {} attempts",
            max_attempts
        );
    }

    pub fn countdown(seconds: u64) {
        use std::io::{self, Write};
        for i in (1..=seconds).rev() {
            eprint!("\r\x1b[33m⏳\x1b[0m Retrying in {}s... ", i);
            let _ = io::stderr().flush();
            std::thread::sleep(Duration::from_secs(1));
        }
        eprint!("\r\x1b[K");
        let _ = io::stderr().flush();
    }
}

pub async fn retry_with_countdown<T, F, Fut>(
    config: &RetryConfig,
    operation: F,
) -> Result<T, AgentError>
where
    F: Fn() -> Fut,
    Fut: Future<Output = Result<T, AgentError>>,
{
    let mut last_error: Option<AgentError> = None;
    let mut successful_attempt = 0u32;

    for attempt in 0..config.max_retries {
        match operation().await {
            Ok(result) => {
                successful_attempt = attempt + 1;
                if attempt > 0 {
                    RetryDisplay::show_retry_success(successful_attempt);
                }
                return Ok(result);
            }
            Err(error) => {
                let retryable = is_retryable(&error);
                let is_last = attempt == config.max_retries - 1;

                if !config.retry_all_errors && retryable.is_none() {
                    return Err(error);
                }

                if is_last {
                    RetryDisplay::show_retry_exhausted(config.max_retries);
                    return Err(error);
                }

                let delay_ms = calculate_delay(config, attempt, &retryable);
                let state = RetryState {
                    attempt: attempt + 1,
                    max_attempts: config.max_retries,
                    delay_ms,
                    error_type: retryable.unwrap_or(RetryableError::NetworkError {
                        message: error.to_string(),
                    }),
                };

                RetryDisplay::show_retry(&state);

                let delay_secs = (delay_ms / 1000).max(1);
                RetryDisplay::countdown(delay_secs);

                last_error = Some(error);
            }
        }
    }

    Err(last_error.unwrap_or_else(|| AgentError::internal("Retry loop exhausted")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    #[test]
    fn test_is_retryable_rate_limit() {
        let error = AgentError::api("429 Too Many Requests");
        assert!(matches!(
            is_retryable(&error),
            Some(RetryableError::RateLimit { .. })
        ));
    }

    #[test]
    fn test_is_retryable_timeout() {
        let error = AgentError::timeout(5000);
        assert!(matches!(is_retryable(&error), Some(RetryableError::Timeout)));
    }

    #[test]
    fn test_not_retryable() {
        let error = AgentError::validation("Invalid input");
        assert!(is_retryable(&error).is_none());
    }

    #[test]
    fn test_retry_state_message() {
        let state = RetryState {
            attempt: 1,
            max_attempts: 3,
            delay_ms: 2000,
            error_type: RetryableError::RateLimit { retry_after_ms: None },
        };

        let msg = state.format_message();
        assert!(msg.contains("Rate limited"));
        assert!(msg.contains("1/3"));
        assert!(msg.contains("2.0s"));
    }

    #[tokio::test]
    async fn test_retry_success_first_attempt() {
        let config = RetryConfig::default();
        let result: Result<i32, AgentError> =
            with_retry(&config, None, || async { Ok(42) }).await;

        assert_eq!(result.unwrap(), 42);
    }

    #[tokio::test]
    async fn test_retry_success_after_failures() {
        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();

        let config = RetryConfig::default().with_base_delay(10);

        let result: Result<i32, AgentError> = with_retry(&config, None, || {
            let c = counter_clone.clone();
            async move {
                let count = c.fetch_add(1, Ordering::SeqCst);
                if count < 2 {
                    Err(AgentError::api("429 rate limit"))
                } else {
                    Ok(42)
                }
            }
        })
        .await;

        assert_eq!(result.unwrap(), 42);
        assert_eq!(counter.load(Ordering::SeqCst), 3);
    }
}
