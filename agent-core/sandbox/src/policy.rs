#![deny(clippy::all)]
#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SandboxPolicy {
    None,
    ReadOnly,
    WorkspaceWrite,
    NetworkBlocked,
    Full,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxConfig {
    pub policy: SandboxPolicy,
    pub allowed_read_paths: Vec<PathBuf>,
    pub allowed_write_paths: Vec<PathBuf>,
    pub allowed_execute_paths: Vec<PathBuf>,
    pub allow_network: bool,
    pub workspace_root: Option<PathBuf>,
}

impl Default for SandboxPolicy {
    fn default() -> Self {
        Self::ReadOnly
    }
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            policy: SandboxPolicy::default(),
            allowed_read_paths: Vec::new(),
            allowed_write_paths: Vec::new(),
            allowed_execute_paths: Vec::new(),
            allow_network: false,
            workspace_root: None,
        }
    }
}

impl SandboxConfig {
    pub fn none() -> Self {
        Self {
            policy: SandboxPolicy::None,
            allow_network: true,
            ..Default::default()
        }
    }

    pub fn read_only() -> Self {
        Self {
            policy: SandboxPolicy::ReadOnly,
            allow_network: false,
            ..Default::default()
        }
    }

    pub fn workspace_write(workspace_root: PathBuf) -> Self {
        Self {
            policy: SandboxPolicy::WorkspaceWrite,
            allowed_write_paths: vec![workspace_root.clone()],
            allowed_read_paths: vec![workspace_root.clone()],
            workspace_root: Some(workspace_root),
            allow_network: false,
            ..Default::default()
        }
    }

    pub fn with_read_path(mut self, path: PathBuf) -> Self {
        self.allowed_read_paths.push(path);
        self
    }

    pub fn with_write_path(mut self, path: PathBuf) -> Self {
        self.allowed_write_paths.push(path);
        self
    }

    pub fn with_execute_path(mut self, path: PathBuf) -> Self {
        self.allowed_execute_paths.push(path);
        self
    }

    pub fn with_network(mut self, allow: bool) -> Self {
        self.allow_network = allow;
        self
    }

    pub fn can_read(&self, path: &std::path::Path) -> bool {
        if self.policy == SandboxPolicy::None {
            return true;
        }

        self.allowed_read_paths
            .iter()
            .any(|allowed| path.starts_with(allowed))
    }

    pub fn can_write(&self, path: &std::path::Path) -> bool {
        if self.policy == SandboxPolicy::None {
            return true;
        }

        if self.policy == SandboxPolicy::ReadOnly {
            return false;
        }

        self.allowed_write_paths
            .iter()
            .any(|allowed| path.starts_with(allowed))
    }

    pub fn can_execute(&self, path: &std::path::Path) -> bool {
        if self.policy == SandboxPolicy::None {
            return true;
        }

        self.allowed_execute_paths
            .iter()
            .any(|allowed| path.starts_with(allowed))
    }

    pub fn can_network(&self) -> bool {
        self.policy == SandboxPolicy::None || self.allow_network
    }
}

impl SandboxPolicy {
    pub fn is_restricted(&self) -> bool {
        !matches!(self, SandboxPolicy::None)
    }

    pub fn allows_write(&self) -> bool {
        matches!(
            self,
            SandboxPolicy::None | SandboxPolicy::WorkspaceWrite | SandboxPolicy::Full
        )
    }

    pub fn allows_network(&self) -> bool {
        !matches!(self, SandboxPolicy::NetworkBlocked | SandboxPolicy::Full)
    }
}
