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
use tokio::sync::{broadcast, mpsc, oneshot, RwLock};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellSession {
    pub id: String,
    pub shell: String,
    pub cwd: PathBuf,
    pub created_at: SystemTime,
    pub last_command: Option<String>,
    pub command_count: usize,
    pub env: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct ShellCommandResult {
    pub command: String,
    pub output: String,
    pub exit_code: Option<i32>,
    pub duration_ms: u64,
    pub is_background: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ShellEvent {
    SessionCreated { id: String },
    SessionClosed { id: String },
    CommandStarted { id: String, command: String },
    CommandOutput { id: String, data: String, is_stderr: bool },
    CommandCompleted { id: String, exit_code: Option<i32> },
    Prompt { id: String, prompt: String },
}

struct InteractiveSession {
    info: ShellSession,
    child: Child,
    stdin_tx: mpsc::Sender<String>,
    output_rx: mpsc::Receiver<String>,
    output_buffer: Vec<String>,
    pending_command: Option<oneshot::Sender<ShellCommandResult>>,
    current_command: Option<String>,
    command_start: Option<SystemTime>,
}

pub struct InteractiveShell {
    sessions: Arc<RwLock<HashMap<String, InteractiveSession>>>,
    event_tx: broadcast::Sender<ShellEvent>,
    default_shell: String,
    default_cwd: PathBuf,
    prompt_patterns: Vec<String>,
    max_buffer_lines: usize,
}

impl InteractiveShell {
    pub fn new(cwd: impl Into<PathBuf>) -> Self {
        let (event_tx, _) = broadcast::channel(256);
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            event_tx,
            default_shell: detect_default_shell(),
            default_cwd: cwd.into(),
            prompt_patterns: vec![
                "$ ".to_string(),
                "# ".to_string(),
                "> ".to_string(),
                "% ".to_string(),
            ],
            max_buffer_lines: 5000,
        }
    }

    pub fn with_shell(mut self, shell: impl Into<String>) -> Self {
        self.default_shell = shell.into();
        self
    }

    pub fn with_prompt_pattern(mut self, pattern: impl Into<String>) -> Self {
        self.prompt_patterns.push(pattern.into());
        self
    }

    pub fn subscribe(&self) -> broadcast::Receiver<ShellEvent> {
        self.event_tx.subscribe()
    }

    pub async fn create_session(&self, id: impl Into<String>) -> AgentResult<ShellSession> {
        let id = id.into();
        let cwd = self.default_cwd.clone();

        let mut child = Command::new(&self.default_shell)
            .current_dir(&cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .env("PS1", "$ ")
            .env("PROMPT", "$ ")
            .spawn()
            .map_err(|e| AgentError::io("spawn shell", e))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| AgentError::internal("failed to get stdin"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| AgentError::internal("failed to get stdout"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| AgentError::internal("failed to get stderr"))?;

        let (stdin_tx, mut stdin_rx) = mpsc::channel::<String>(32);
        let (output_tx, output_rx) = mpsc::channel::<String>(256);

        let session_info = ShellSession {
            id: id.clone(),
            shell: self.default_shell.clone(),
            cwd,
            created_at: SystemTime::now(),
            last_command: None,
            command_count: 0,
            env: HashMap::new(),
        };

        let event_tx = self.event_tx.clone();
        let _session_id = id.clone();

        tokio::spawn(async move {
            let mut stdin_writer = stdin;
            while let Some(input) = stdin_rx.recv().await {
                if stdin_writer.write_all(input.as_bytes()).await.is_err() {
                    break;
                }
                if stdin_writer.write_all(b"\n").await.is_err() {
                    break;
                }
                let _ = stdin_writer.flush().await;
            }
        });

        let output_tx2 = output_tx.clone();
        let event_tx2 = event_tx.clone();
        let session_id2 = id.clone();

        tokio::spawn(async move {
            let reader = BufReader::new(stdout);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let _ = output_tx2.send(line.clone()).await;
                let _ = event_tx2.send(ShellEvent::CommandOutput {
                    id: session_id2.clone(),
                    data: line,
                    is_stderr: false,
                });
            }
        });

        let session_id3 = id.clone();
        let event_tx3 = event_tx.clone();

        tokio::spawn(async move {
            let reader = BufReader::new(stderr);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let _ = output_tx.send(format!("[stderr] {}", line)).await;
                let _ = event_tx3.send(ShellEvent::CommandOutput {
                    id: session_id3.clone(),
                    data: line,
                    is_stderr: true,
                });
            }
        });

        let session = InteractiveSession {
            info: session_info.clone(),
            child,
            stdin_tx,
            output_rx,
            output_buffer: Vec::new(),
            pending_command: None,
            current_command: None,
            command_start: None,
        };

        self.sessions.write().await.insert(id.clone(), session);

        let _ = self.event_tx.send(ShellEvent::SessionCreated { id });

        Ok(session_info)
    }

    pub async fn execute(
        &self,
        session_id: &str,
        command: impl Into<String>,
        timeout: Duration,
    ) -> AgentResult<ShellCommandResult> {
        let command = command.into();

        {
            let mut sessions = self.sessions.write().await;
            let session = sessions
                .get_mut(session_id)
                .ok_or_else(|| AgentError::not_found(format!("session: {}", session_id)))?;

            session.current_command = Some(command.clone());
            session.command_start = Some(SystemTime::now());
            session.info.command_count += 1;
            session.info.last_command = Some(command.clone());

            session
                .stdin_tx
                .send(command.clone())
                .await
                .map_err(|_| AgentError::internal("stdin closed"))?;
        }

        let _ = self.event_tx.send(ShellEvent::CommandStarted {
            id: session_id.to_string(),
            command: command.clone(),
        });

        let echo_marker = format!("__EXIT_CODE_{}__", uuid::Uuid::new_v4());
        {
            let sessions = self.sessions.read().await;
            if let Some(session) = sessions.get(session_id) {
                let _ = session
                    .stdin_tx
                    .send(format!("echo {}$?", echo_marker))
                    .await;
            }
        }

        let start = std::time::Instant::now();
        let mut output_lines = Vec::new();
        let mut exit_code = None;

        loop {
            if start.elapsed() > timeout {
                return Err(AgentError::timeout(timeout.as_millis() as u64));
            }

            let mut got_output = false;

            {
                let mut sessions = self.sessions.write().await;
                if let Some(session) = sessions.get_mut(session_id) {
                    while let Ok(line) = session.output_rx.try_recv() {
                        got_output = true;

                        if line.contains(&echo_marker) {
                            let code_str = line.replace(&echo_marker, "");
                            exit_code = code_str.trim().parse().ok();
                            break;
                        }

                        output_lines.push(line.clone());
                        session.output_buffer.push(line);

                        if session.output_buffer.len() > self.max_buffer_lines {
                            session.output_buffer.remove(0);
                        }
                    }
                }
            }

            if exit_code.is_some() {
                break;
            }

            if !got_output {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        }

        let duration_ms = start.elapsed().as_millis() as u64;

        let _ = self.event_tx.send(ShellEvent::CommandCompleted {
            id: session_id.to_string(),
            exit_code,
        });

        Ok(ShellCommandResult {
            command,
            output: output_lines.join("\n"),
            exit_code,
            duration_ms,
            is_background: false,
        })
    }

    pub async fn send_input(&self, session_id: &str, input: &str) -> AgentResult<()> {
        let sessions = self.sessions.read().await;
        let session = sessions
            .get(session_id)
            .ok_or_else(|| AgentError::not_found(format!("session: {}", session_id)))?;

        session
            .stdin_tx
            .send(input.to_string())
            .await
            .map_err(|_| AgentError::internal("stdin closed"))
    }

    pub async fn send_signal(&self, session_id: &str, signal: Signal) -> AgentResult<()> {
        let input = match signal {
            Signal::Interrupt => "\x03",
            Signal::Eof => "\x04",
            Signal::Suspend => "\x1a",
        };

        self.send_input(session_id, input).await
    }

    pub async fn get_output(&self, session_id: &str, last_n: Option<usize>) -> AgentResult<String> {
        let sessions = self.sessions.read().await;
        let session = sessions
            .get(session_id)
            .ok_or_else(|| AgentError::not_found(format!("session: {}", session_id)))?;

        let lines = match last_n {
            Some(n) => session
                .output_buffer
                .iter()
                .rev()
                .take(n)
                .rev()
                .cloned()
                .collect::<Vec<_>>(),
            None => session.output_buffer.clone(),
        };

        Ok(lines.join("\n"))
    }

    pub async fn close_session(&self, session_id: &str) -> AgentResult<()> {
        let mut sessions = self.sessions.write().await;

        if let Some(mut session) = sessions.remove(session_id) {
            let _ = session.stdin_tx.send("exit".to_string()).await;
            let _ = session.child.kill().await;
        }

        let _ = self.event_tx.send(ShellEvent::SessionClosed {
            id: session_id.to_string(),
        });

        Ok(())
    }

    pub async fn list_sessions(&self) -> Vec<ShellSession> {
        self.sessions
            .read()
            .await
            .values()
            .map(|s| s.info.clone())
            .collect()
    }

    pub async fn get_session(&self, session_id: &str) -> Option<ShellSession> {
        self.sessions
            .read()
            .await
            .get(session_id)
            .map(|s| s.info.clone())
    }

    pub async fn change_directory(&self, session_id: &str, path: &str) -> AgentResult<()> {
        let result = self
            .execute(session_id, format!("cd {} && pwd", path), Duration::from_secs(5))
            .await?;

        if result.exit_code != Some(0) {
            return Err(AgentError::internal(format!(
                "failed to change directory: {}",
                result.output
            )));
        }

        let new_cwd = result.output.trim().to_string();

        let mut sessions = self.sessions.write().await;
        if let Some(session) = sessions.get_mut(session_id) {
            session.info.cwd = PathBuf::from(new_cwd);
        }

        Ok(())
    }

    pub async fn set_env(&self, session_id: &str, key: &str, value: &str) -> AgentResult<()> {
        let cmd = format!("export {}={}", key, shell_escape(value));
        self.execute(session_id, cmd, Duration::from_secs(5))
            .await?;

        let mut sessions = self.sessions.write().await;
        if let Some(session) = sessions.get_mut(session_id) {
            session.info.env.insert(key.to_string(), value.to_string());
        }

        Ok(())
    }
}

impl Default for InteractiveShell {
    fn default() -> Self {
        Self::new(".")
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Signal {
    Interrupt,
    Eof,
    Suspend,
}

fn detect_default_shell() -> String {
    if cfg!(target_os = "windows") {
        std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".to_string())
    } else {
        std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string())
    }
}

fn shell_escape(s: &str) -> String {
    if s.chars()
        .all(|c| c.is_alphanumeric() || c == '_' || c == '-' || c == '.' || c == '/')
    {
        s.to_string()
    } else {
        format!("'{}'", s.replace('\'', "'\"'\"'"))
    }
}
