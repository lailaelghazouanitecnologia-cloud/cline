#![deny(clippy::all)]
#![forbid(unsafe_code)]

use crate::registry::HookRegistry;
use crate::types::{HookContext, HookDefinition, HookEvent, HookResult};
use agent_common::{AgentError, AgentResult};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Instant;
use tokio::process::Command;
use tokio::sync::RwLock;
use tokio::time::{timeout, Duration};

pub struct HookExecutor {
    registry: Arc<RwLock<HookRegistry>>,
}

impl HookExecutor {
    pub fn new(registry: HookRegistry) -> Self {
        Self {
            registry: Arc::new(RwLock::new(registry)),
        }
    }

    pub async fn register(&self, hook: HookDefinition) {
        let mut registry = self.registry.write().await;
        registry.register(hook);
    }

    pub async fn unregister(&self, name: &str) {
        let mut registry = self.registry.write().await;
        registry.unregister(name);
    }

    pub async fn execute(&self, context: &HookContext) -> AgentResult<Vec<HookResult>> {
        let registry = self.registry.read().await;
        let hooks = registry.get(context.event);

        if hooks.is_empty() {
            return Ok(Vec::new());
        }

        let mut results = Vec::new();

        for hook in hooks {
            let result = self.execute_hook(hook, context).await?;

            if result.blocked && hook.fail_on_error {
                results.push(result);
                return Err(AgentError::rejected_with_reason(format!(
                    "hook {} blocked execution",
                    hook.name
                )));
            }

            results.push(result);
        }

        Ok(results)
    }

    pub async fn has_hooks_for(&self, event: HookEvent) -> bool {
        let registry = self.registry.read().await;
        registry.has_hooks_for(event)
    }

    async fn execute_hook(&self, hook: &HookDefinition, context: &HookContext) -> AgentResult<HookResult> {
        let start = Instant::now();

        let context_json = serde_json::to_string(&context)
            .map_err(|e| AgentError::serialization(e.to_string()))?;

        let mut cmd = Command::new("sh");
        cmd.arg("-c")
            .arg(&hook.command)
            .current_dir(&context.working_dir)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .env("AGENT_HOOK_EVENT", context.event.to_string())
            .env("AGENT_HOOK_CONTEXT", &context_json)
            .env("AGENT_SESSION_ID", &context.session_id);

        if let Some(ref env_vars) = hook.env {
            for var in env_vars {
                cmd.env(&var.key, &var.value);
            }
        }

        let timeout_duration = Duration::from_millis(hook.timeout_ms);

        let output = match timeout(timeout_duration, cmd.output()).await {
            Ok(result) => result.map_err(|e| AgentError::io("hook execution", e))?,
            Err(_) => {
                return Ok(HookResult::blocked(
                    &hook.name,
                    context.event,
                    format!("hook timed out after {}ms", hook.timeout_ms),
                    start.elapsed().as_millis() as u64,
                ));
            }
        };

        let duration_ms = start.elapsed().as_millis() as u64;
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let exit_code = output.status.code().unwrap_or(-1);

        if output.status.success() {
            Ok(HookResult::success(
                &hook.name,
                context.event,
                exit_code,
                stdout,
                stderr,
                duration_ms,
            ))
        } else if stderr.contains("BLOCKED:") {
            let blocked_msg = stderr
                .lines()
                .find(|line| line.starts_with("BLOCKED:"))
                .map(|line| line.trim_start_matches("BLOCKED:").trim().to_string())
                .unwrap_or_else(|| "hook blocked execution".to_string());

            Ok(HookResult::blocked(
                &hook.name,
                context.event,
                blocked_msg,
                duration_ms,
            ))
        } else {
            Ok(HookResult::failure(
                &hook.name,
                context.event,
                Some(exit_code),
                stdout,
                stderr,
                duration_ms,
            ))
        }
    }
}

impl Default for HookExecutor {
    fn default() -> Self {
        Self::new(HookRegistry::default())
    }
}

impl std::fmt::Display for HookEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HookEvent::SessionStart => write!(f, "session_start"),
            HookEvent::SessionEnd => write!(f, "session_end"),
            HookEvent::BeforeToolCall => write!(f, "before_tool_call"),
            HookEvent::AfterToolCall => write!(f, "after_tool_call"),
            HookEvent::BeforeMessage => write!(f, "before_message"),
            HookEvent::AfterMessage => write!(f, "after_message"),
            HookEvent::OnError => write!(f, "on_error"),
            HookEvent::UserPromptSubmit => write!(f, "user_prompt_submit"),
        }
    }
}
