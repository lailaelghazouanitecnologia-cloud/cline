use std::process::Stdio;
use tokio::process::Command;

use crate::context::ToolContext;
use crate::handler::{ToolFuture, ToolHandler};
use crate::spec::{ToolCall, ToolOutput, ToolSpec};

pub struct ShellHandler;

impl ToolHandler for ShellHandler {
    fn spec(&self) -> ToolSpec {
        ToolSpec::new("shell", "Execute a shell command")
            .with_parameter("command", "string", "The shell command to execute", true)
            .with_parameter("timeout_ms", "integer", "Timeout in milliseconds", false)
    }

    fn execute(&self, context: &ToolContext, call: ToolCall) -> ToolFuture {
        let command = call.get_string("command").unwrap_or_default();
        let timeout_ms = call.get_u64("timeout_ms").unwrap_or(context.timeout_ms());
        let working_directory = context.working_directory().clone();

        Box::pin(async move {
            let shell = if cfg!(windows) { "cmd" } else { "sh" };
            let shell_arg = if cfg!(windows) { "/C" } else { "-c" };

            let child = Command::new(shell)
                .arg(shell_arg)
                .arg(&command)
                .current_dir(&working_directory)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn();

            let child = match child {
                Ok(child) => child,
                Err(error) => {
                    return Ok(ToolOutput::failure(error.to_string()));
                }
            };

            let timeout = tokio::time::Duration::from_millis(timeout_ms);
            let output_result = tokio::time::timeout(timeout, child.wait_with_output()).await;

            match output_result {
                Ok(Ok(output)) => {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    let exit_code = output.status.code().unwrap_or(-1);

                    let content = format!(
                        "exit code: {}\n\nstdout:\n{}\n\nstderr:\n{}",
                        exit_code, stdout, stderr
                    );

                    if output.status.success() {
                        Ok(ToolOutput::success(content))
                    } else {
                        Ok(ToolOutput::failure(content))
                    }
                }
                Ok(Err(error)) => Ok(ToolOutput::failure(error.to_string())),
                Err(_) => {
                    let message = format!("command timed out after {}ms", timeout_ms);
                    Ok(ToolOutput::failure(message))
                }
            }
        })
    }

    fn is_dangerous(&self, _call: &ToolCall) -> bool {
        true
    }
}
