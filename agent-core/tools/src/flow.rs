#![deny(clippy::all)]
#![forbid(unsafe_code)]

use crate::approval::{ApprovalManager, ApprovalRequest, ApprovalResponse, ApprovalReason, ApprovalContext, RiskLevel};
use agent_common::AgentResult;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, RwLock};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolExecution {
    pub id: String,
    pub tool_name: String,
    pub input: serde_json::Value,
    pub status: ExecutionStatus,
    pub created_at: u64,
    pub started_at: Option<u64>,
    pub completed_at: Option<u64>,
    pub result: Option<serde_json::Value>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionStatus {
    Pending,
    WaitingApproval,
    Approved,
    Denied,
    Running,
    Completed,
    Failed,
    Cancelled,
    Timeout,
}

#[derive(Debug, Clone)]
pub enum FlowEvent {
    ExecutionQueued(String),
    ApprovalRequired(String, ApprovalRequest),
    ApprovalReceived(String, ApprovalResponse),
    ExecutionStarted(String),
    ExecutionCompleted(String, serde_json::Value),
    ExecutionFailed(String, String),
    ExecutionCancelled(String),
    QueueDrained,
}

pub struct ExecutionFlow {
    queue: Arc<RwLock<VecDeque<ToolExecution>>>,
    active: Arc<RwLock<HashMap<String, ToolExecution>>>,
    history: Arc<RwLock<Vec<ToolExecution>>>,
    approval_manager: Arc<ApprovalManager>,
    event_sender: Option<mpsc::Sender<FlowEvent>>,
    id_counter: AtomicU64,
    max_concurrent: usize,
    max_history: usize,
    default_timeout: Duration,
}

impl ExecutionFlow {
    pub fn new(approval_manager: Arc<ApprovalManager>) -> Self {
        Self {
            queue: Arc::new(RwLock::new(VecDeque::new())),
            active: Arc::new(RwLock::new(HashMap::new())),
            history: Arc::new(RwLock::new(Vec::new())),
            approval_manager,
            event_sender: None,
            id_counter: AtomicU64::new(0),
            max_concurrent: 1,
            max_history: 100,
            default_timeout: Duration::from_secs(300),
        }
    }

    fn next_id(&self, prefix: &str) -> String {
        let n = self.id_counter.fetch_add(1, Ordering::SeqCst);
        format!("{}-{}", prefix, n)
    }

    pub fn with_max_concurrent(mut self, max: usize) -> Self {
        self.max_concurrent = max;
        self
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.default_timeout = timeout;
        self
    }

    pub fn subscribe(&mut self) -> mpsc::Receiver<FlowEvent> {
        let (tx, rx) = mpsc::channel(100);
        self.event_sender = Some(tx);
        rx
    }

    pub async fn enqueue(
        &self,
        tool_name: &str,
        input: serde_json::Value,
    ) -> AgentResult<String> {
        let id = self.next_id("exec");
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let execution = ToolExecution {
            id: id.clone(),
            tool_name: tool_name.to_string(),
            input,
            status: ExecutionStatus::Pending,
            created_at: now,
            started_at: None,
            completed_at: None,
            result: None,
            error: None,
        };

        self.queue.write().await.push_back(execution);
        self.send_event(FlowEvent::ExecutionQueued(id.clone())).await;

        Ok(id)
    }

    pub async fn process_next(&self) -> Option<ToolExecution> {
        let active_count = self.active.read().await.len();
        if active_count >= self.max_concurrent {
            return None;
        }

        let mut queue = self.queue.write().await;
        let mut execution = queue.pop_front()?;

        execution.status = ExecutionStatus::WaitingApproval;
        drop(queue);

        let approval_request = self.build_approval_request(&execution);

        self.send_event(FlowEvent::ApprovalRequired(
            execution.id.clone(),
            approval_request.clone(),
        ))
        .await;

        let response = self.approval_manager.request_approval(approval_request).await;

        self.send_event(FlowEvent::ApprovalReceived(
            execution.id.clone(),
            response,
        ))
        .await;

        if response.is_approved() {
            execution.status = ExecutionStatus::Approved;
            self.active.write().await.insert(execution.id.clone(), execution.clone());
            Some(execution)
        } else {
            execution.status = ExecutionStatus::Denied;
            self.add_to_history(execution).await;
            None
        }
    }

    fn build_approval_request(&self, execution: &ToolExecution) -> ApprovalRequest {
        let reason = self.determine_reason(&execution.tool_name, &execution.input);
        let risk = self.assess_risk(&execution.tool_name, &execution.input);

        ApprovalRequest {
            id: execution.id.clone(),
            tool_name: execution.tool_name.clone(),
            tool_input: execution.input.clone(),
            reason,
            context: ApprovalContext {
                file_paths: self.extract_paths(&execution.input),
                command: self.extract_command(&execution.input),
                description: format!("Execute {} tool", execution.tool_name),
                risk_level: risk,
            },
        }
    }

    fn determine_reason(&self, tool_name: &str, input: &serde_json::Value) -> ApprovalReason {
        match tool_name {
            "execute_command" | "shell" | "bash" => ApprovalReason::CommandExecution,
            "write_file" | "edit_file" | "apply_patch" => ApprovalReason::FileModification,
            "fetch_url" | "web_search" => ApprovalReason::NetworkAccess,
            _ => {
                if input.get("path").is_some() {
                    if let Some(path) = input.get("path").and_then(|p| p.as_str()) {
                        if path.contains(".env") || path.contains(".ssh") || path.contains("/etc/") {
                            return ApprovalReason::SensitivePath;
                        }
                    }
                }
                ApprovalReason::Custom
            }
        }
    }

    fn assess_risk(&self, tool_name: &str, input: &serde_json::Value) -> RiskLevel {
        match tool_name {
            "execute_command" | "shell" | "bash" => {
                if let Some(cmd) = input.get("command").and_then(|c| c.as_str()) {
                    if cmd.contains("rm ") || cmd.contains("sudo") || cmd.contains("chmod") {
                        return RiskLevel::High;
                    }
                    if cmd.contains("git push") || cmd.contains("npm publish") {
                        return RiskLevel::High;
                    }
                }
                RiskLevel::Medium
            }
            "write_file" | "edit_file" => {
                if let Some(path) = input.get("path").and_then(|p| p.as_str()) {
                    if path.contains(".env") || path.contains("config") || path.contains("secret") {
                        return RiskLevel::High;
                    }
                }
                RiskLevel::Medium
            }
            "read_file" | "list_files" | "search_files" => RiskLevel::Low,
            _ => RiskLevel::Medium,
        }
    }

    fn extract_paths(&self, input: &serde_json::Value) -> Vec<String> {
        let mut paths = Vec::new();
        if let Some(path) = input.get("path").and_then(|p| p.as_str()) {
            paths.push(path.to_string());
        }
        if let Some(file) = input.get("file").and_then(|f| f.as_str()) {
            paths.push(file.to_string());
        }
        if let Some(files) = input.get("files").and_then(|f| f.as_array()) {
            for f in files {
                if let Some(s) = f.as_str() {
                    paths.push(s.to_string());
                }
            }
        }
        paths
    }

    fn extract_command(&self, input: &serde_json::Value) -> Option<String> {
        input.get("command").and_then(|c| c.as_str()).map(String::from)
    }

    pub async fn mark_started(&self, id: &str) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        if let Some(execution) = self.active.write().await.get_mut(id) {
            execution.status = ExecutionStatus::Running;
            execution.started_at = Some(now);
        }

        self.send_event(FlowEvent::ExecutionStarted(id.to_string())).await;
    }

    pub async fn mark_completed(&self, id: &str, result: serde_json::Value) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        if let Some(mut execution) = self.active.write().await.remove(id) {
            execution.status = ExecutionStatus::Completed;
            execution.completed_at = Some(now);
            execution.result = Some(result.clone());
            self.add_to_history(execution).await;
        }

        self.send_event(FlowEvent::ExecutionCompleted(id.to_string(), result)).await;
        self.check_queue_drained().await;
    }

    pub async fn mark_failed(&self, id: &str, error: String) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        if let Some(mut execution) = self.active.write().await.remove(id) {
            execution.status = ExecutionStatus::Failed;
            execution.completed_at = Some(now);
            execution.error = Some(error.clone());
            self.add_to_history(execution).await;
        }

        self.send_event(FlowEvent::ExecutionFailed(id.to_string(), error)).await;
        self.check_queue_drained().await;
    }

    pub async fn cancel(&self, id: &str) -> bool {
        let mut queue = self.queue.write().await;
        if let Some(pos) = queue.iter().position(|e| e.id == id) {
            let mut execution = queue.remove(pos).unwrap();
            execution.status = ExecutionStatus::Cancelled;
            drop(queue);
            self.add_to_history(execution).await;
            self.send_event(FlowEvent::ExecutionCancelled(id.to_string())).await;
            return true;
        }

        if let Some(mut execution) = self.active.write().await.remove(id) {
            execution.status = ExecutionStatus::Cancelled;
            self.add_to_history(execution).await;
            self.send_event(FlowEvent::ExecutionCancelled(id.to_string())).await;
            return true;
        }

        false
    }

    pub async fn cancel_all(&self) {
        let mut queue = self.queue.write().await;
        while let Some(mut execution) = queue.pop_front() {
            execution.status = ExecutionStatus::Cancelled;
            let id = execution.id.clone();
            drop(queue);
            self.add_to_history(execution).await;
            self.send_event(FlowEvent::ExecutionCancelled(id)).await;
            queue = self.queue.write().await;
        }

        let mut active = self.active.write().await;
        let ids: Vec<_> = active.keys().cloned().collect();
        for id in ids {
            if let Some(mut execution) = active.remove(&id) {
                execution.status = ExecutionStatus::Cancelled;
                drop(active);
                self.add_to_history(execution).await;
                self.send_event(FlowEvent::ExecutionCancelled(id)).await;
                active = self.active.write().await;
            }
        }
    }

    async fn add_to_history(&self, execution: ToolExecution) {
        let mut history = self.history.write().await;
        history.push(execution);
        while history.len() > self.max_history {
            history.remove(0);
        }
    }

    async fn check_queue_drained(&self) {
        let queue_empty = self.queue.read().await.is_empty();
        let active_empty = self.active.read().await.is_empty();

        if queue_empty && active_empty {
            self.send_event(FlowEvent::QueueDrained).await;
        }
    }

    async fn send_event(&self, event: FlowEvent) {
        if let Some(ref tx) = self.event_sender {
            let _ = tx.send(event).await;
        }
    }

    pub async fn queue_size(&self) -> usize {
        self.queue.read().await.len()
    }

    pub async fn active_count(&self) -> usize {
        self.active.read().await.len()
    }

    pub async fn get_history(&self) -> Vec<ToolExecution> {
        self.history.read().await.clone()
    }

    pub async fn get_execution(&self, id: &str) -> Option<ToolExecution> {
        if let Some(exec) = self.active.read().await.get(id) {
            return Some(exec.clone());
        }

        if let Some(exec) = self.queue.read().await.iter().find(|e| e.id == id) {
            return Some(exec.clone());
        }

        self.history.read().await.iter().find(|e| e.id == id).cloned()
    }
}

pub struct BatchExecutor {
    flow: Arc<ExecutionFlow>,
    batch_id: String,
    executions: Vec<String>,
}

static BATCH_COUNTER: AtomicU64 = AtomicU64::new(0);

impl BatchExecutor {
    pub fn new(flow: Arc<ExecutionFlow>) -> Self {
        let n = BATCH_COUNTER.fetch_add(1, Ordering::SeqCst);
        Self {
            flow,
            batch_id: format!("batch-{}", n),
            executions: Vec::new(),
        }
    }

    pub async fn add(&mut self, tool_name: &str, input: serde_json::Value) -> AgentResult<String> {
        let id = self.flow.enqueue(tool_name, input).await?;
        self.executions.push(id.clone());
        Ok(id)
    }

    pub fn batch_id(&self) -> &str {
        &self.batch_id
    }

    pub fn execution_ids(&self) -> &[String] {
        &self.executions
    }

    pub async fn cancel_all(&self) {
        for id in &self.executions {
            self.flow.cancel(id).await;
        }
    }
}
