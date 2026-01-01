#![deny(clippy::all)]
#![forbid(unsafe_code)]

use agent_common::{AgentError, AgentResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{broadcast, mpsc, RwLock};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminalState {
    Idle,
    Running,
    WaitingInput,
    Completed,
    Failed,
    Killed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessInfo {
    pub id: String,
    pub pid: Option<u32>,
    pub command: String,
    pub state: TerminalState,
    pub exit_code: Option<i32>,
    pub started_at: SystemTime,
    pub completed_at: Option<SystemTime>,
    pub cwd: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TerminalEvent {
    Started { id: String, pid: u32 },
    Output { id: String, data: String, is_stderr: bool },
    Completed { id: String, exit_code: i32 },
    Failed { id: String, error: String },
    Killed { id: String },
}

struct TerminalProcess {
    info: ProcessInfo,
    child: Option<Child>,
    stdin_tx: Option<mpsc::Sender<String>>,
    output_buffer: Vec<String>,
}

pub struct TerminalManager {
    sessions: Arc<RwLock<HashMap<String, TerminalProcess>>>,
    default_cwd: PathBuf,
    default_shell: String,
    env_vars: HashMap<String, String>,
    event_tx: broadcast::Sender<TerminalEvent>,
    max_output_lines: usize,
}

impl TerminalManager {
    pub fn new(cwd: impl Into<PathBuf>) -> Self {
        let (event_tx, _) = broadcast::channel(256);
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            default_cwd: cwd.into(),
            default_shell: detect_shell(),
            env_vars: HashMap::new(),
            event_tx,
            max_output_lines: 1000,
        }
    }

    pub fn with_shell(mut self, shell: impl Into<String>) -> Self {
        self.default_shell = shell.into();
        self
    }

    pub fn with_env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env_vars.insert(key.into(), value.into());
        self
    }

    pub fn subscribe(&self) -> broadcast::Receiver<TerminalEvent> {
        self.event_tx.subscribe()
    }

    pub async fn spawn(
        &self,
        id: impl Into<String>,
        command: impl Into<String>,
        cwd: Option<PathBuf>,
    ) -> AgentResult<ProcessInfo> {
        let id = id.into();
        let command = command.into();
        let cwd = cwd.unwrap_or_else(|| self.default_cwd.clone());

        let (stdin_tx, mut stdin_rx) = mpsc::channel::<String>(32);

        let mut child = Command::new(&self.default_shell)
            .arg("-c")
            .arg(&command)
            .current_dir(&cwd)
            .envs(&self.env_vars)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| AgentError::io("spawn process", e))?;

        let pid = child.id().unwrap_or(0);

        let info = ProcessInfo {
            id: id.clone(),
            pid: Some(pid),
            command: command.clone(),
            state: TerminalState::Running,
            exit_code: None,
            started_at: SystemTime::now(),
            completed_at: None,
            cwd,
        };

        let _ = self.event_tx.send(TerminalEvent::Started {
            id: id.clone(),
            pid,
        });

        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        let mut stdin = child.stdin.take();

        let sessions = Arc::clone(&self.sessions);
        let event_tx = self.event_tx.clone();
        let process_id = id.clone();
        let max_lines = self.max_output_lines;

        tokio::spawn(async move {
            if let Some(stdout) = stdout {
                let reader = BufReader::new(stdout);
                let mut lines = reader.lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    let mut sessions = sessions.write().await;
                    if let Some(proc) = sessions.get_mut(&process_id) {
                        proc.output_buffer.push(line.clone());
                        if proc.output_buffer.len() > max_lines {
                            proc.output_buffer.remove(0);
                        }
                    }
                    let _ = event_tx.send(TerminalEvent::Output {
                        id: process_id.clone(),
                        data: line,
                        is_stderr: false,
                    });
                }
            }
        });

        let sessions2 = Arc::clone(&self.sessions);
        let event_tx2 = self.event_tx.clone();
        let process_id2 = id.clone();
        let max_lines2 = self.max_output_lines;

        tokio::spawn(async move {
            if let Some(stderr) = stderr {
                let reader = BufReader::new(stderr);
                let mut lines = reader.lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    let mut sessions = sessions2.write().await;
                    if let Some(proc) = sessions.get_mut(&process_id2) {
                        proc.output_buffer.push(format!("[stderr] {}", line));
                        if proc.output_buffer.len() > max_lines2 {
                            proc.output_buffer.remove(0);
                        }
                    }
                    let _ = event_tx2.send(TerminalEvent::Output {
                        id: process_id2.clone(),
                        data: line,
                        is_stderr: true,
                    });
                }
            }
        });

        let process_id3 = id.clone();
        tokio::spawn(async move {
            while let Some(input) = stdin_rx.recv().await {
                if let Some(ref mut stdin_writer) = stdin {
                    let _ = stdin_writer.write_all(input.as_bytes()).await;
                    let _ = stdin_writer.write_all(b"\n").await;
                    let _ = stdin_writer.flush().await;
                }
            }
            drop(stdin);
            let _ = process_id3;
        });

        let process = TerminalProcess {
            info: info.clone(),
            child: Some(child),
            stdin_tx: Some(stdin_tx),
            output_buffer: Vec::new(),
        };

        self.sessions.write().await.insert(id.clone(), process);

        self.spawn_wait_task(id).await;

        Ok(info)
    }

    async fn spawn_wait_task(&self, id: String) {
        let sessions = Arc::clone(&self.sessions);
        let event_tx = self.event_tx.clone();

        tokio::spawn(async move {
            let mut child_opt = None;
            {
                let mut sessions = sessions.write().await;
                if let Some(proc) = sessions.get_mut(&id) {
                    child_opt = proc.child.take();
                }
            }

            if let Some(mut child) = child_opt {
                let status = child.wait().await;

                let mut sessions = sessions.write().await;
                if let Some(proc) = sessions.get_mut(&id) {
                    match status {
                        Ok(exit) => {
                            let code = exit.code().unwrap_or(-1);
                            proc.info.exit_code = Some(code);
                            proc.info.state = TerminalState::Completed;
                            proc.info.completed_at = Some(SystemTime::now());
                            let _ = event_tx.send(TerminalEvent::Completed {
                                id: id.clone(),
                                exit_code: code,
                            });
                        }
                        Err(e) => {
                            proc.info.state = TerminalState::Failed;
                            proc.info.completed_at = Some(SystemTime::now());
                            let _ = event_tx.send(TerminalEvent::Failed {
                                id: id.clone(),
                                error: e.to_string(),
                            });
                        }
                    }
                }
            }
        });
    }

    pub async fn send_input(&self, id: &str, input: &str) -> AgentResult<()> {
        let sessions = self.sessions.read().await;
        let proc = sessions
            .get(id)
            .ok_or_else(|| AgentError::not_found(format!("terminal: {}", id)))?;

        if let Some(tx) = &proc.stdin_tx {
            tx.send(input.to_string())
                .await
                .map_err(|_| AgentError::internal("stdin channel closed"))?;
        }
        Ok(())
    }

    pub async fn kill(&self, id: &str) -> AgentResult<()> {
        let mut sessions = self.sessions.write().await;
        let proc = sessions
            .get_mut(id)
            .ok_or_else(|| AgentError::not_found(format!("terminal: {}", id)))?;

        if let Some(ref mut child) = proc.child {
            child
                .kill()
                .await
                .map_err(|e| AgentError::io("kill process", e))?;
        }

        proc.info.state = TerminalState::Killed;
        proc.info.completed_at = Some(SystemTime::now());

        let _ = self.event_tx.send(TerminalEvent::Killed { id: id.to_string() });

        Ok(())
    }

    pub async fn get_output(&self, id: &str) -> AgentResult<Vec<String>> {
        let sessions = self.sessions.read().await;
        let proc = sessions
            .get(id)
            .ok_or_else(|| AgentError::not_found(format!("terminal: {}", id)))?;

        Ok(proc.output_buffer.clone())
    }

    pub async fn get_info(&self, id: &str) -> Option<ProcessInfo> {
        self.sessions.read().await.get(id).map(|p| p.info.clone())
    }

    pub async fn list_active(&self) -> Vec<ProcessInfo> {
        self.sessions
            .read()
            .await
            .values()
            .filter(|p| p.info.state == TerminalState::Running)
            .map(|p| p.info.clone())
            .collect()
    }

    pub async fn list_all(&self) -> Vec<ProcessInfo> {
        self.sessions
            .read()
            .await
            .values()
            .map(|p| p.info.clone())
            .collect()
    }

    pub async fn cleanup_completed(&self, max_age: Duration) -> usize {
        let now = SystemTime::now();
        let mut to_remove = Vec::new();

        {
            let sessions = self.sessions.read().await;
            for (id, proc) in sessions.iter() {
                if matches!(
                    proc.info.state,
                    TerminalState::Completed | TerminalState::Failed | TerminalState::Killed
                ) {
                    if let Some(completed) = proc.info.completed_at {
                        if let Ok(elapsed) = now.duration_since(completed) {
                            if elapsed > max_age {
                                to_remove.push(id.clone());
                            }
                        }
                    }
                }
            }
        }

        let count = to_remove.len();
        let mut sessions = self.sessions.write().await;
        for id in to_remove {
            sessions.remove(&id);
        }

        count
    }

    pub async fn wait_for(&self, id: &str, timeout: Duration) -> AgentResult<ProcessInfo> {
        let start = SystemTime::now();

        loop {
            if let Some(info) = self.get_info(id).await {
                if !matches!(info.state, TerminalState::Running | TerminalState::WaitingInput) {
                    return Ok(info);
                }
            } else {
                return Err(AgentError::not_found(format!("terminal: {}", id)));
            }

            if SystemTime::now().duration_since(start).unwrap_or_default() > timeout {
                return Err(AgentError::timeout(timeout.as_millis() as u64));
            }

            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    pub async fn run_command(
        &self,
        command: impl Into<String>,
        timeout: Duration,
    ) -> AgentResult<(i32, String)> {
        let id = uuid::Uuid::new_v4().to_string();
        let command = command.into();

        self.spawn(&id, &command, None).await?;

        let info = self.wait_for(&id, timeout).await?;
        let output = self.get_output(&id).await?.join("\n");
        let exit_code = info.exit_code.unwrap_or(-1);

        Ok((exit_code, output))
    }
}

impl Default for TerminalManager {
    fn default() -> Self {
        Self::new(".")
    }
}

fn detect_shell() -> String {
    if cfg!(target_os = "windows") {
        std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".to_string())
    } else {
        std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string())
    }
}
