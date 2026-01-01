#![deny(clippy::all)]
#![forbid(unsafe_code)]

use crate::persistence::ProgressTracker;
use agent_common::{AgentError, AgentResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, RwLock};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskDefinition {
    pub id: String,
    pub name: String,
    pub description: String,
    pub steps: Vec<TaskStep>,
    pub max_retries: u32,
    pub timeout_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskStep {
    pub id: String,
    pub name: String,
    pub action: StepAction,
    pub depends_on: Vec<String>,
    pub retry_on_fail: bool,
    pub max_retries: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StepAction {
    LlmQuery { prompt: String },
    ToolCall { tool_name: String, input: serde_json::Value },
    Conditional { condition: String, then_step: String, else_step: Option<String> },
    Parallel { steps: Vec<String> },
    WaitApproval { message: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepStatus {
    Pending,
    Running,
    WaitingDependency,
    Completed,
    Failed,
    Skipped,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepResult {
    pub step_id: String,
    pub status: StepStatus,
    pub output: Option<serde_json::Value>,
    pub error: Option<String>,
    pub duration_ms: u64,
    pub attempts: u32,
}

#[derive(Debug, Clone)]
pub enum TaskEvent {
    TaskStarted { task_id: String },
    StepStarted { task_id: String, step_id: String },
    StepCompleted { task_id: String, step_id: String, result: StepResult },
    StepFailed { task_id: String, step_id: String, error: String },
    StepRetrying { task_id: String, step_id: String, attempt: u32 },
    TaskCompleted { task_id: String, results: HashMap<String, StepResult> },
    TaskFailed { task_id: String, error: String },
    ProgressUpdate { task_id: String, completed: usize, total: usize },
}

pub struct TaskRunner {
    task: TaskDefinition,
    step_results: Arc<RwLock<HashMap<String, StepResult>>>,
    step_status: Arc<RwLock<HashMap<String, StepStatus>>>,
    event_sender: Option<mpsc::Sender<TaskEvent>>,
    started_at: Option<Instant>,
    progress: ProgressTracker,
}

impl TaskRunner {
    pub fn new(task: TaskDefinition) -> Self {
        let mut progress = ProgressTracker::new();
        progress.set_steps(task.steps.iter().map(|s| s.name.clone()).collect());

        Self {
            task,
            step_results: Arc::new(RwLock::new(HashMap::new())),
            step_status: Arc::new(RwLock::new(HashMap::new())),
            event_sender: None,
            started_at: None,
            progress,
        }
    }

    pub fn subscribe(&mut self) -> mpsc::Receiver<TaskEvent> {
        let (tx, rx) = mpsc::channel(100);
        self.event_sender = Some(tx);
        rx
    }

    pub async fn run<E>(&mut self, executor: &E) -> AgentResult<HashMap<String, StepResult>>
    where
        E: StepExecutor,
    {
        self.started_at = Some(Instant::now());
        let task_id = self.task.id.clone();

        self.send_event(TaskEvent::TaskStarted { task_id: task_id.clone() }).await;

        for step in &self.task.steps {
            self.step_status.write().await.insert(step.id.clone(), StepStatus::Pending);
        }

        let execution_order = self.topological_sort()?;

        for step_id in execution_order {
            let step = self.task.steps.iter().find(|s| s.id == step_id)
                .ok_or_else(|| AgentError::not_found(&step_id))?
                .clone();

            if !self.dependencies_met(&step).await {
                self.step_status.write().await.insert(step.id.clone(), StepStatus::Skipped);
                continue;
            }

            let result = self.execute_step(&step, executor).await;

            match &result {
                Ok(r) if r.status == StepStatus::Completed => {
                    self.send_event(TaskEvent::StepCompleted {
                        task_id: task_id.clone(),
                        step_id: step.id.clone(),
                        result: r.clone(),
                    }).await;
                }
                Ok(r) if r.status == StepStatus::Failed => {
                    self.send_event(TaskEvent::StepFailed {
                        task_id: task_id.clone(),
                        step_id: step.id.clone(),
                        error: r.error.clone().unwrap_or_default(),
                    }).await;

                    if !step.retry_on_fail {
                        return Err(AgentError::api(
                            r.error.clone().unwrap_or_else(|| "Step failed".to_string())
                        ));
                    }
                }
                Err(e) => {
                    let error_msg = e.to_string();
                    self.send_event(TaskEvent::TaskFailed {
                        task_id: task_id.clone(),
                        error: error_msg.clone(),
                    }).await;
                    return Err(AgentError::api(error_msg));
                }
                _ => {}
            }

            if let Ok(r) = result {
                self.step_results.write().await.insert(step.id.clone(), r);
            }

            let completed = self.step_results.read().await.len();
            let total = self.task.steps.len();
            self.send_event(TaskEvent::ProgressUpdate {
                task_id: task_id.clone(),
                completed,
                total,
            }).await;
        }

        let results = self.step_results.read().await.clone();

        self.send_event(TaskEvent::TaskCompleted {
            task_id,
            results: results.clone(),
        }).await;

        Ok(results)
    }

    async fn execute_step<E>(&mut self, step: &TaskStep, executor: &E) -> AgentResult<StepResult>
    where
        E: StepExecutor,
    {
        let start = Instant::now();
        let step_id = step.id.clone();
        let max_retries = if step.retry_on_fail { step.max_retries } else { 0 };

        self.step_status.write().await.insert(step_id.clone(), StepStatus::Running);

        self.send_event(TaskEvent::StepStarted {
            task_id: self.task.id.clone(),
            step_id: step_id.clone(),
        }).await;

        let mut last_error = None;

        for attempt in 0..=max_retries {
            if attempt > 0 {
                self.send_event(TaskEvent::StepRetrying {
                    task_id: self.task.id.clone(),
                    step_id: step_id.clone(),
                    attempt,
                }).await;

                tokio::time::sleep(Duration::from_millis(100 * 2u64.pow(attempt))).await;
            }

            match executor.execute(&step.action).await {
                Ok(output) => {
                    self.step_status.write().await.insert(step_id.clone(), StepStatus::Completed);

                    return Ok(StepResult {
                        step_id,
                        status: StepStatus::Completed,
                        output: Some(output),
                        error: None,
                        duration_ms: start.elapsed().as_millis() as u64,
                        attempts: attempt + 1,
                    });
                }
                Err(e) => {
                    last_error = Some(e.to_string());
                }
            }
        }

        self.step_status.write().await.insert(step_id.clone(), StepStatus::Failed);

        Ok(StepResult {
            step_id,
            status: StepStatus::Failed,
            output: None,
            error: last_error,
            duration_ms: start.elapsed().as_millis() as u64,
            attempts: max_retries + 1,
        })
    }

    async fn dependencies_met(&self, step: &TaskStep) -> bool {
        let results = self.step_results.read().await;

        for dep_id in &step.depends_on {
            match results.get(dep_id) {
                Some(r) if r.status == StepStatus::Completed => continue,
                _ => return false,
            }
        }

        true
    }

    fn topological_sort(&self) -> AgentResult<Vec<String>> {
        let mut result = Vec::new();
        let mut visited = HashMap::new();
        let mut in_stack = HashMap::new();

        for step in &self.task.steps {
            if !visited.contains_key(&step.id) {
                self.dfs_sort(&step.id, &mut visited, &mut in_stack, &mut result)?;
            }
        }

        result.reverse();
        Ok(result)
    }

    fn dfs_sort(
        &self,
        step_id: &str,
        visited: &mut HashMap<String, bool>,
        in_stack: &mut HashMap<String, bool>,
        result: &mut Vec<String>,
    ) -> AgentResult<()> {
        visited.insert(step_id.to_string(), true);
        in_stack.insert(step_id.to_string(), true);

        if let Some(step) = self.task.steps.iter().find(|s| s.id == step_id) {
            for dep_id in &step.depends_on {
                if in_stack.get(dep_id).copied().unwrap_or(false) {
                    return Err(AgentError::api(format!("Circular dependency: {} -> {}", step_id, dep_id)));
                }
                if !visited.get(dep_id).copied().unwrap_or(false) {
                    self.dfs_sort(dep_id, visited, in_stack, result)?;
                }
            }
        }

        in_stack.insert(step_id.to_string(), false);
        result.push(step_id.to_string());

        Ok(())
    }

    async fn send_event(&self, event: TaskEvent) {
        if let Some(ref tx) = self.event_sender {
            let _ = tx.send(event).await;
        }
    }

    pub fn progress(&self) -> &ProgressTracker {
        &self.progress
    }

    pub async fn get_results(&self) -> HashMap<String, StepResult> {
        self.step_results.read().await.clone()
    }
}

#[async_trait::async_trait]
pub trait StepExecutor: Send + Sync {
    async fn execute(&self, action: &StepAction) -> AgentResult<serde_json::Value>;
}

pub struct TaskBuilder {
    id: String,
    name: String,
    description: String,
    steps: Vec<TaskStep>,
    max_retries: u32,
    timeout_secs: u64,
    step_counter: u32,
}

impl TaskBuilder {
    pub fn new(name: impl Into<String>) -> Self {
        let name = name.into();
        let id = format!("task-{}", uuid_simple());

        Self {
            id,
            name,
            description: String::new(),
            steps: Vec::new(),
            max_retries: 3,
            timeout_secs: 600,
            step_counter: 0,
        }
    }

    pub fn description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }

    pub fn max_retries(mut self, retries: u32) -> Self {
        self.max_retries = retries;
        self
    }

    pub fn timeout(mut self, secs: u64) -> Self {
        self.timeout_secs = secs;
        self
    }

    pub fn add_step(mut self, name: impl Into<String>, action: StepAction) -> Self {
        self.step_counter += 1;
        let step = TaskStep {
            id: format!("step-{}", self.step_counter),
            name: name.into(),
            action,
            depends_on: Vec::new(),
            retry_on_fail: true,
            max_retries: 3,
        };
        self.steps.push(step);
        self
    }

    pub fn add_step_with_deps(
        mut self,
        name: impl Into<String>,
        action: StepAction,
        deps: Vec<String>,
    ) -> Self {
        self.step_counter += 1;
        let step = TaskStep {
            id: format!("step-{}", self.step_counter),
            name: name.into(),
            action,
            depends_on: deps,
            retry_on_fail: true,
            max_retries: 3,
        };
        self.steps.push(step);
        self
    }

    pub fn llm_step(self, name: impl Into<String>, prompt: impl Into<String>) -> Self {
        self.add_step(name, StepAction::LlmQuery { prompt: prompt.into() })
    }

    pub fn tool_step(self, name: impl Into<String>, tool: impl Into<String>, input: serde_json::Value) -> Self {
        self.add_step(name, StepAction::ToolCall {
            tool_name: tool.into(),
            input,
        })
    }

    pub fn approval_step(self, name: impl Into<String>, message: impl Into<String>) -> Self {
        self.add_step(name, StepAction::WaitApproval { message: message.into() })
    }

    pub fn build(self) -> TaskDefinition {
        TaskDefinition {
            id: self.id,
            name: self.name,
            description: self.description,
            steps: self.steps,
            max_retries: self.max_retries,
            timeout_secs: self.timeout_secs,
        }
    }
}

fn uuid_simple() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{:x}", timestamp)
}

pub struct SimpleExecutor;

#[async_trait::async_trait]
impl StepExecutor for SimpleExecutor {
    async fn execute(&self, action: &StepAction) -> AgentResult<serde_json::Value> {
        match action {
            StepAction::LlmQuery { prompt } => {
                Ok(serde_json::json!({ "prompt": prompt, "response": "Placeholder response" }))
            }
            StepAction::ToolCall { tool_name, input } => {
                Ok(serde_json::json!({ "tool": tool_name, "input": input, "output": "Placeholder output" }))
            }
            StepAction::WaitApproval { message } => {
                Ok(serde_json::json!({ "approved": true, "message": message }))
            }
            StepAction::Conditional { condition, then_step, else_step } => {
                Ok(serde_json::json!({
                    "condition": condition,
                    "then": then_step,
                    "else": else_step
                }))
            }
            StepAction::Parallel { steps } => {
                Ok(serde_json::json!({ "parallel_steps": steps }))
            }
        }
    }
}
