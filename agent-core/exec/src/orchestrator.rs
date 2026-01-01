#![deny(clippy::all)]
#![forbid(unsafe_code)]

use agent_common::{AgentError, AgentResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, RwLock};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandSpec {
    pub id: String,
    pub command: String,
    pub args: Vec<String>,
    pub working_dir: Option<PathBuf>,
    pub env: HashMap<String, String>,
    pub timeout: Option<Duration>,
    pub depends_on: Vec<String>,
    pub on_failure: FailureAction,
    pub capture_output: bool,
    pub background: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureAction {
    Continue,
    Stop,
    Retry,
    Rollback,
}

impl Default for FailureAction {
    fn default() -> Self {
        Self::Stop
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandResult {
    pub id: String,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub duration_ms: u64,
    pub success: bool,
}

#[derive(Debug, Clone)]
pub enum OrchestratorEvent {
    CommandStarted { id: String },
    CommandOutput { id: String, line: String, is_stderr: bool },
    CommandCompleted { id: String, result: CommandResult },
    CommandFailed { id: String, error: String },
    BatchCompleted { total: usize, succeeded: usize, failed: usize },
}

pub struct CommandOrchestrator {
    commands: Vec<CommandSpec>,
    results: Arc<RwLock<HashMap<String, CommandResult>>>,
    event_sender: Option<mpsc::Sender<OrchestratorEvent>>,
    max_parallel: usize,
    default_timeout: Duration,
    working_dir: PathBuf,
}

impl CommandOrchestrator {
    pub fn new() -> Self {
        Self {
            commands: Vec::new(),
            results: Arc::new(RwLock::new(HashMap::new())),
            event_sender: None,
            max_parallel: 4,
            default_timeout: Duration::from_secs(300),
            working_dir: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
        }
    }

    pub fn with_max_parallel(mut self, max: usize) -> Self {
        self.max_parallel = max;
        self
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.default_timeout = timeout;
        self
    }

    pub fn with_working_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.working_dir = dir.into();
        self
    }

    pub fn subscribe(&mut self) -> mpsc::Receiver<OrchestratorEvent> {
        let (tx, rx) = mpsc::channel(100);
        self.event_sender = Some(tx);
        rx
    }

    pub fn add_command(&mut self, spec: CommandSpec) {
        self.commands.push(spec);
    }

    pub fn add_simple(&mut self, id: &str, command: &str) {
        let parts: Vec<&str> = command.split_whitespace().collect();
        if parts.is_empty() {
            return;
        }

        self.commands.push(CommandSpec {
            id: id.to_string(),
            command: parts[0].to_string(),
            args: parts[1..].iter().map(|s| s.to_string()).collect(),
            working_dir: None,
            env: HashMap::new(),
            timeout: None,
            depends_on: Vec::new(),
            on_failure: FailureAction::Stop,
            capture_output: true,
            background: false,
        });
    }

    pub fn add_chain(&mut self, commands: Vec<(&str, &str)>) {
        let mut prev_id: Option<String> = None;

        for (id, cmd) in commands {
            let parts: Vec<&str> = cmd.split_whitespace().collect();
            if parts.is_empty() {
                continue;
            }

            let depends_on = prev_id.map(|id| vec![id]).unwrap_or_default();

            self.commands.push(CommandSpec {
                id: id.to_string(),
                command: parts[0].to_string(),
                args: parts[1..].iter().map(|s| s.to_string()).collect(),
                working_dir: None,
                env: HashMap::new(),
                timeout: None,
                depends_on,
                on_failure: FailureAction::Stop,
                capture_output: true,
                background: false,
            });

            prev_id = Some(id.to_string());
        }
    }

    pub async fn execute(&mut self) -> AgentResult<BatchResult> {
        let mut completed = 0;
        let mut failed = 0;
        let total = self.commands.len();

        let execution_order = self.topological_sort()?;

        for id in execution_order {
            let spec = self.commands.iter().find(|c| c.id == id).cloned();

            if let Some(spec) = spec {
                if !self.dependencies_satisfied(&spec).await {
                    failed += 1;
                    continue;
                }

                self.send_event(OrchestratorEvent::CommandStarted { id: id.clone() })
                    .await;

                match self.execute_command(&spec).await {
                    Ok(result) => {
                        let success = result.success;
                        self.results.write().await.insert(id.clone(), result.clone());

                        self.send_event(OrchestratorEvent::CommandCompleted {
                            id: id.clone(),
                            result,
                        })
                        .await;

                        if success {
                            completed += 1;
                        } else {
                            failed += 1;
                            if spec.on_failure == FailureAction::Stop {
                                break;
                            }
                        }
                    }
                    Err(e) => {
                        failed += 1;
                        self.send_event(OrchestratorEvent::CommandFailed {
                            id: id.clone(),
                            error: e.to_string(),
                        })
                        .await;

                        if spec.on_failure == FailureAction::Stop {
                            break;
                        }
                    }
                }
            }
        }

        self.send_event(OrchestratorEvent::BatchCompleted {
            total,
            succeeded: completed,
            failed,
        })
        .await;

        Ok(BatchResult {
            total,
            succeeded: completed,
            failed,
            results: self.results.read().await.clone(),
        })
    }

    async fn execute_command(&self, spec: &CommandSpec) -> AgentResult<CommandResult> {
        let start = std::time::Instant::now();
        let working_dir = spec.working_dir.as_ref().unwrap_or(&self.working_dir);

        let mut cmd = tokio::process::Command::new(&spec.command);
        cmd.args(&spec.args)
            .current_dir(working_dir)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        for (key, value) in &spec.env {
            cmd.env(key, value);
        }

        let timeout = spec.timeout.unwrap_or(self.default_timeout);

        let output = tokio::time::timeout(timeout, cmd.output())
            .await
            .map_err(|_| AgentError::timeout(timeout.as_secs()))?
            .map_err(|e| AgentError::tool_execution(&spec.command, e.to_string()))?;

        let duration_ms = start.elapsed().as_millis() as u64;

        Ok(CommandResult {
            id: spec.id.clone(),
            exit_code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            duration_ms,
            success: output.status.success(),
        })
    }

    async fn dependencies_satisfied(&self, spec: &CommandSpec) -> bool {
        let results = self.results.read().await;

        for dep_id in &spec.depends_on {
            match results.get(dep_id) {
                Some(result) if result.success => continue,
                _ => return false,
            }
        }

        true
    }

    fn topological_sort(&self) -> AgentResult<Vec<String>> {
        let mut sorted = Vec::new();
        let mut visited = std::collections::HashSet::new();
        let mut temp_visited = std::collections::HashSet::new();

        for cmd in &self.commands {
            if !visited.contains(&cmd.id) {
                self.visit(&cmd.id, &mut visited, &mut temp_visited, &mut sorted)?;
            }
        }

        Ok(sorted)
    }

    fn visit(
        &self,
        id: &str,
        visited: &mut std::collections::HashSet<String>,
        temp: &mut std::collections::HashSet<String>,
        sorted: &mut Vec<String>,
    ) -> AgentResult<()> {
        if temp.contains(id) {
            return Err(AgentError::validation("Circular dependency detected"));
        }

        if visited.contains(id) {
            return Ok(());
        }

        temp.insert(id.to_string());

        if let Some(cmd) = self.commands.iter().find(|c| c.id == id) {
            for dep in &cmd.depends_on {
                self.visit(dep, visited, temp, sorted)?;
            }
        }

        temp.remove(id);
        visited.insert(id.to_string());
        sorted.push(id.to_string());

        Ok(())
    }

    async fn send_event(&self, event: OrchestratorEvent) {
        if let Some(ref tx) = self.event_sender {
            let _ = tx.send(event).await;
        }
    }

    pub fn clear(&mut self) {
        self.commands.clear();
    }

    pub async fn get_results(&self) -> HashMap<String, CommandResult> {
        self.results.read().await.clone()
    }

    pub async fn get_result(&self, id: &str) -> Option<CommandResult> {
        self.results.read().await.get(id).cloned()
    }
}

impl Default for CommandOrchestrator {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct BatchResult {
    pub total: usize,
    pub succeeded: usize,
    pub failed: usize,
    pub results: HashMap<String, CommandResult>,
}

impl BatchResult {
    pub fn all_succeeded(&self) -> bool {
        self.failed == 0 && self.succeeded == self.total
    }

    pub fn summary(&self) -> String {
        format!(
            "{}/{} commands succeeded, {} failed",
            self.succeeded, self.total, self.failed
        )
    }
}
