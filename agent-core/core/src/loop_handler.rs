#![deny(clippy::all)]
#![forbid(unsafe_code)]

use agent_common::{AgentError, AgentResult};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, RwLock};

#[derive(Debug, Clone)]
pub enum LoopEvent {
    TurnStarted { turn_number: u32 },
    MessageReceived { content: String },
    ToolCallStarted { id: String, name: String },
    ToolCallCompleted { id: String, name: String, output: String },
    ToolCallFailed { id: String, name: String, error: String },
    TurnCompleted { turn_number: u32, finish_reason: FinishReason },
    LoopCompleted { total_turns: u32, reason: CompletionReason },
    LoopAborted { reason: String },
    ApprovalRequired { tool_name: String, input: serde_json::Value },
    ApprovalGranted { tool_name: String },
    ApprovalDenied { tool_name: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FinishReason {
    Stop,
    ToolUse,
    MaxTokens,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompletionReason {
    NaturalEnd,
    MaxTurns,
    UserInterrupt,
    Error,
    Timeout,
}

#[derive(Debug, Clone)]
pub struct LoopConfig {
    pub max_turns: u32,
    pub turn_timeout: Duration,
    pub total_timeout: Duration,
    pub auto_approve_tools: Vec<String>,
    pub require_approval: bool,
    pub continue_on_error: bool,
}

impl Default for LoopConfig {
    fn default() -> Self {
        Self {
            max_turns: 50,
            turn_timeout: Duration::from_secs(300),
            total_timeout: Duration::from_secs(3600),
            auto_approve_tools: vec![
                "read_file".to_string(),
                "list_files".to_string(),
                "search_files".to_string(),
            ],
            require_approval: true,
            continue_on_error: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallRequest {
    pub id: String,
    pub name: String,
    pub input: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallResponse {
    pub id: String,
    pub output: String,
    pub is_error: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnResult {
    pub turn_number: u32,
    pub assistant_message: Option<String>,
    pub tool_calls: Vec<ToolCallRequest>,
    pub tool_results: Vec<ToolCallResponse>,
    pub finish_reason: FinishReason,
    pub duration_ms: u64,
}

#[async_trait::async_trait]
pub trait ToolExecutor: Send + Sync {
    async fn execute(&self, name: &str, input: serde_json::Value) -> AgentResult<String>;
}

pub struct ConversationLoop<T: ToolExecutor> {
    config: LoopConfig,
    executor: Arc<T>,
    event_sender: Option<mpsc::Sender<LoopEvent>>,
    approval_sender: Option<mpsc::Sender<(String, serde_json::Value)>>,
    approval_receiver: Option<mpsc::Receiver<bool>>,
    turn_count: u32,
    started_at: Option<Instant>,
    is_running: Arc<RwLock<bool>>,
}

impl<T: ToolExecutor> ConversationLoop<T> {
    pub fn new(executor: Arc<T>, config: LoopConfig) -> Self {
        Self {
            config,
            executor,
            event_sender: None,
            approval_sender: None,
            approval_receiver: None,
            turn_count: 0,
            started_at: None,
            is_running: Arc::new(RwLock::new(false)),
        }
    }

    pub fn subscribe_events(&mut self) -> mpsc::Receiver<LoopEvent> {
        let (tx, rx) = mpsc::channel(100);
        self.event_sender = Some(tx);
        rx
    }

    pub fn setup_approval(&mut self) -> (mpsc::Receiver<(String, serde_json::Value)>, mpsc::Sender<bool>) {
        let (approval_tx, approval_rx) = mpsc::channel(10);
        let (response_tx, response_rx) = mpsc::channel(10);
        self.approval_sender = Some(approval_tx);
        self.approval_receiver = Some(response_rx);
        (approval_rx, response_tx)
    }

    pub async fn process_turn<F, Fut>(&mut self, assistant_handler: F) -> AgentResult<TurnResult>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = AgentResult<AssistantResponse>>,
    {
        let turn_start = Instant::now();
        self.turn_count += 1;
        let turn_number = self.turn_count;

        self.send_event(LoopEvent::TurnStarted { turn_number }).await;

        if self.turn_count > self.config.max_turns {
            self.send_event(LoopEvent::LoopCompleted {
                total_turns: self.turn_count - 1,
                reason: CompletionReason::MaxTurns,
            }).await;

            return Err(AgentError::api("Max turns exceeded"));
        }

        let response = assistant_handler().await?;

        if let Some(ref text) = response.text {
            self.send_event(LoopEvent::MessageReceived { content: text.clone() }).await;
        }

        let mut tool_results = Vec::new();

        for tool_call in &response.tool_calls {
            let result = self.execute_tool_call(tool_call).await;
            tool_results.push(result);
        }

        let finish_reason = if response.tool_calls.is_empty() {
            FinishReason::Stop
        } else {
            FinishReason::ToolUse
        };

        self.send_event(LoopEvent::TurnCompleted { turn_number, finish_reason }).await;

        Ok(TurnResult {
            turn_number,
            assistant_message: response.text,
            tool_calls: response.tool_calls,
            tool_results,
            finish_reason,
            duration_ms: turn_start.elapsed().as_millis() as u64,
        })
    }

    async fn execute_tool_call(&mut self, call: &ToolCallRequest) -> ToolCallResponse {
        self.send_event(LoopEvent::ToolCallStarted {
            id: call.id.clone(),
            name: call.name.clone(),
        }).await;

        if self.config.require_approval && !self.is_auto_approved(&call.name) {
            let approved = self.request_approval(&call.name, &call.input).await;

            if !approved {
                self.send_event(LoopEvent::ApprovalDenied { tool_name: call.name.clone() }).await;

                return ToolCallResponse {
                    id: call.id.clone(),
                    output: "Tool execution denied by user".to_string(),
                    is_error: true,
                };
            }

            self.send_event(LoopEvent::ApprovalGranted { tool_name: call.name.clone() }).await;
        }

        match self.executor.execute(&call.name, call.input.clone()).await {
            Ok(output) => {
                self.send_event(LoopEvent::ToolCallCompleted {
                    id: call.id.clone(),
                    name: call.name.clone(),
                    output: output.clone(),
                }).await;

                ToolCallResponse {
                    id: call.id.clone(),
                    output,
                    is_error: false,
                }
            }
            Err(e) => {
                self.send_event(LoopEvent::ToolCallFailed {
                    id: call.id.clone(),
                    name: call.name.clone(),
                    error: e.to_string(),
                }).await;

                ToolCallResponse {
                    id: call.id.clone(),
                    output: e.to_string(),
                    is_error: true,
                }
            }
        }
    }

    fn is_auto_approved(&self, tool_name: &str) -> bool {
        self.config.auto_approve_tools.contains(&tool_name.to_string())
    }

    async fn request_approval(&mut self, tool_name: &str, input: &serde_json::Value) -> bool {
        self.send_event(LoopEvent::ApprovalRequired {
            tool_name: tool_name.to_string(),
            input: input.clone(),
        }).await;

        if let Some(ref tx) = self.approval_sender {
            if tx.send((tool_name.to_string(), input.clone())).await.is_ok() {
                if let Some(ref mut rx) = self.approval_receiver {
                    if let Some(approved) = rx.recv().await {
                        return approved;
                    }
                }
            }
        }

        false
    }

    pub async fn run_loop<F, Fut>(&mut self, mut assistant_handler: F) -> AgentResult<LoopSummary>
    where
        F: FnMut(Option<Vec<ToolCallResponse>>) -> Fut,
        Fut: std::future::Future<Output = AgentResult<AssistantResponse>>,
    {
        self.started_at = Some(Instant::now());
        *self.is_running.write().await = true;

        let mut all_turns = Vec::new();
        let mut last_tool_results: Option<Vec<ToolCallResponse>> = None;

        loop {
            if !*self.is_running.read().await {
                self.send_event(LoopEvent::LoopAborted {
                    reason: "Interrupted".to_string(),
                }).await;
                break;
            }

            if let Some(started) = self.started_at {
                if started.elapsed() > self.config.total_timeout {
                    self.send_event(LoopEvent::LoopCompleted {
                        total_turns: self.turn_count,
                        reason: CompletionReason::Timeout,
                    }).await;
                    break;
                }
            }

            let turn_results = last_tool_results.take();

            let handler = || async {
                assistant_handler(turn_results).await
            };

            match self.process_turn(handler).await {
                Ok(result) => {
                    let is_final = result.finish_reason == FinishReason::Stop;
                    last_tool_results = Some(result.tool_results.clone());
                    all_turns.push(result);

                    if is_final {
                        self.send_event(LoopEvent::LoopCompleted {
                            total_turns: self.turn_count,
                            reason: CompletionReason::NaturalEnd,
                        }).await;
                        break;
                    }
                }
                Err(e) => {
                    if self.config.continue_on_error {
                        continue;
                    }

                    self.send_event(LoopEvent::LoopAborted {
                        reason: e.to_string(),
                    }).await;

                    return Err(e);
                }
            }
        }

        *self.is_running.write().await = false;

        let total_duration = self.started_at
            .map(|s| s.elapsed().as_millis() as u64)
            .unwrap_or(0);

        Ok(LoopSummary {
            total_turns: self.turn_count,
            turns: all_turns,
            total_duration_ms: total_duration,
            completion_reason: CompletionReason::NaturalEnd,
        })
    }

    pub async fn abort(&self) {
        *self.is_running.write().await = false;
    }

    pub fn turn_count(&self) -> u32 {
        self.turn_count
    }

    async fn send_event(&self, event: LoopEvent) {
        if let Some(ref tx) = self.event_sender {
            let _ = tx.send(event).await;
        }
    }
}

#[derive(Debug, Clone)]
pub struct AssistantResponse {
    pub text: Option<String>,
    pub tool_calls: Vec<ToolCallRequest>,
    pub finish_reason: FinishReason,
}

impl AssistantResponse {
    pub fn text_only(text: impl Into<String>) -> Self {
        Self {
            text: Some(text.into()),
            tool_calls: Vec::new(),
            finish_reason: FinishReason::Stop,
        }
    }

    pub fn with_tool_calls(text: Option<String>, calls: Vec<ToolCallRequest>) -> Self {
        Self {
            text,
            tool_calls: calls,
            finish_reason: FinishReason::ToolUse,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoopSummary {
    pub total_turns: u32,
    pub turns: Vec<TurnResult>,
    pub total_duration_ms: u64,
    pub completion_reason: CompletionReason,
}

impl LoopSummary {
    pub fn tool_call_count(&self) -> usize {
        self.turns.iter().map(|t| t.tool_calls.len()).sum()
    }

    pub fn error_count(&self) -> usize {
        self.turns.iter()
            .flat_map(|t| t.tool_results.iter())
            .filter(|r| r.is_error)
            .count()
    }

    pub fn final_message(&self) -> Option<&str> {
        self.turns.last().and_then(|t| t.assistant_message.as_deref())
    }
}

pub struct NoOpExecutor;

#[async_trait::async_trait]
impl ToolExecutor for NoOpExecutor {
    async fn execute(&self, name: &str, _input: serde_json::Value) -> AgentResult<String> {
        Ok(format!("Tool '{}' executed (no-op)", name))
    }
}
