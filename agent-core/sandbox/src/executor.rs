#![deny(clippy::all)]
#![forbid(unsafe_code)]

use crate::policy::{SandboxConfig, SandboxPolicy};
use agent_common::{AgentError, AgentResult};
use std::path::Path;

pub struct SandboxExecutor {
    config: SandboxConfig,
}

impl SandboxExecutor {
    pub fn new(config: SandboxConfig) -> Self {
        Self { config }
    }

    pub fn config(&self) -> &SandboxConfig {
        &self.config
    }

    pub fn policy(&self) -> SandboxPolicy {
        self.config.policy
    }

    pub fn validate_read(&self, path: &Path) -> AgentResult<()> {
        if !self.config.can_read(path) {
            return Err(AgentError::rejected_with_reason(format!(
                "sandbox policy denies read access to: {}",
                path.display()
            )));
        }
        Ok(())
    }

    pub fn validate_write(&self, path: &Path) -> AgentResult<()> {
        if !self.config.can_write(path) {
            return Err(AgentError::rejected_with_reason(format!(
                "sandbox policy denies write access to: {}",
                path.display()
            )));
        }
        Ok(())
    }

    pub fn validate_execute(&self, path: &Path) -> AgentResult<()> {
        if !self.config.can_execute(path) {
            return Err(AgentError::rejected_with_reason(format!(
                "sandbox policy denies execute access to: {}",
                path.display()
            )));
        }
        Ok(())
    }

    pub fn validate_network(&self) -> AgentResult<()> {
        if !self.config.can_network() {
            return Err(AgentError::rejected_with_reason(
                "sandbox policy denies network access",
            ));
        }
        Ok(())
    }

    pub fn validate_command(&self, command: &str) -> AgentResult<()> {
        if self.config.policy == SandboxPolicy::None {
            return Ok(());
        }

        let dangerous_patterns = [
            "rm -rf /",
            "rm -rf /*",
            "mkfs",
            "dd if=/dev/zero",
            ":(){:|:&};:",
            "chmod -R 777 /",
            "chown -R",
            "> /dev/sda",
            "curl | sh",
            "wget | sh",
            "curl | bash",
            "wget | bash",
        ];

        let command_lower = command.to_lowercase();
        for pattern in &dangerous_patterns {
            if command_lower.contains(pattern) {
                return Err(AgentError::rejected_with_reason(format!(
                    "sandbox policy blocks dangerous command pattern: {}",
                    pattern
                )));
            }
        }

        Ok(())
    }

    pub fn is_path_in_workspace(&self, path: &Path) -> bool {
        if let Some(ref workspace_root) = self.config.workspace_root {
            return path.starts_with(workspace_root);
        }
        false
    }

    pub fn workspace_root(&self) -> Option<&Path> {
        self.config.workspace_root.as_deref()
    }
}

impl Default for SandboxExecutor {
    fn default() -> Self {
        Self::new(SandboxConfig::default())
    }
}

#[cfg(target_os = "linux")]
pub mod linux {
    use super::*;

    pub fn is_landlock_supported() -> bool {
        true
    }

    pub fn apply_landlock_rules(_config: &SandboxConfig) -> AgentResult<()> {
        Ok(())
    }
}

#[cfg(not(target_os = "linux"))]
pub mod linux {
    use super::*;

    pub fn is_landlock_supported() -> bool {
        false
    }

    pub fn apply_landlock_rules(_config: &SandboxConfig) -> AgentResult<()> {
        Ok(())
    }
}
