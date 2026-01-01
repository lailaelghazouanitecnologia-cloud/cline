#![deny(clippy::all)]
#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookEvent {
    SessionStart,
    SessionEnd,
    BeforeToolCall,
    AfterToolCall,
    BeforeMessage,
    AfterMessage,
    OnError,
    UserPromptSubmit,
}

impl HookEvent {
    pub fn all() -> Vec<Self> {
        vec![
            Self::SessionStart,
            Self::SessionEnd,
            Self::BeforeToolCall,
            Self::AfterToolCall,
            Self::BeforeMessage,
            Self::AfterMessage,
            Self::OnError,
            Self::UserPromptSubmit,
        ]
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookDefinition {
    pub name: String,
    pub event: HookEvent,
    pub command: String,
    pub enabled: bool,
    pub timeout_ms: u64,
    pub fail_on_error: bool,
    pub env: Option<Vec<EnvVar>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvVar {
    pub key: String,
    pub value: String,
}

impl HookDefinition {
    pub fn new(name: impl Into<String>, event: HookEvent, command: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            event,
            command: command.into(),
            enabled: true,
            timeout_ms: 30000,
            fail_on_error: false,
            env: None,
        }
    }

    pub fn with_timeout(mut self, timeout_ms: u64) -> Self {
        self.timeout_ms = timeout_ms;
        self
    }

    pub fn with_fail_on_error(mut self, fail: bool) -> Self {
        self.fail_on_error = fail;
        self
    }

    pub fn with_env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        let env = self.env.get_or_insert(Vec::new());
        env.push(EnvVar {
            key: key.into(),
            value: value.into(),
        });
        self
    }

    pub fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookContext {
    pub event: HookEvent,
    pub session_id: String,
    pub working_dir: String,
    pub data: serde_json::Value,
}

impl HookContext {
    pub fn new(event: HookEvent, session_id: impl Into<String>, working_dir: impl Into<String>) -> Self {
        Self {
            event,
            session_id: session_id.into(),
            working_dir: working_dir.into(),
            data: serde_json::Value::Null,
        }
    }

    pub fn with_data(mut self, data: serde_json::Value) -> Self {
        self.data = data;
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookResult {
    pub hook_name: String,
    pub event: HookEvent,
    pub success: bool,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub duration_ms: u64,
    pub blocked: bool,
    pub blocked_message: Option<String>,
}

impl HookResult {
    pub fn success(
        hook_name: impl Into<String>,
        event: HookEvent,
        exit_code: i32,
        stdout: String,
        stderr: String,
        duration_ms: u64,
    ) -> Self {
        Self {
            hook_name: hook_name.into(),
            event,
            success: true,
            exit_code: Some(exit_code),
            stdout,
            stderr,
            duration_ms,
            blocked: false,
            blocked_message: None,
        }
    }

    pub fn failure(
        hook_name: impl Into<String>,
        event: HookEvent,
        exit_code: Option<i32>,
        stdout: String,
        stderr: String,
        duration_ms: u64,
    ) -> Self {
        Self {
            hook_name: hook_name.into(),
            event,
            success: false,
            exit_code,
            stdout,
            stderr,
            duration_ms,
            blocked: false,
            blocked_message: None,
        }
    }

    pub fn blocked(
        hook_name: impl Into<String>,
        event: HookEvent,
        message: impl Into<String>,
        duration_ms: u64,
    ) -> Self {
        Self {
            hook_name: hook_name.into(),
            event,
            success: false,
            exit_code: None,
            stdout: String::new(),
            stderr: String::new(),
            duration_ms,
            blocked: true,
            blocked_message: Some(message.into()),
        }
    }
}
