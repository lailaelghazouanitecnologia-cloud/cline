#![deny(clippy::all)]
#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot, RwLock};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalRequest {
    pub id: String,
    pub tool_name: String,
    pub tool_input: serde_json::Value,
    pub reason: ApprovalReason,
    pub context: ApprovalContext,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalReason {
    DangerousTool,
    FileModification,
    CommandExecution,
    NetworkAccess,
    FirstTimeUse,
    SensitivePath,
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalContext {
    pub file_paths: Vec<String>,
    pub command: Option<String>,
    pub description: String,
    pub risk_level: RiskLevel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalResponse {
    Approved,
    Denied,
    ApprovedOnce,
    ApprovedForSession,
    ApprovedForTool,
}

impl ApprovalResponse {
    pub fn is_approved(&self) -> bool {
        matches!(
            self,
            Self::Approved | Self::ApprovedOnce | Self::ApprovedForSession | Self::ApprovedForTool
        )
    }
}

pub struct ApprovalManager {
    pending: Arc<RwLock<HashMap<String, oneshot::Sender<ApprovalResponse>>>>,
    request_tx: mpsc::Sender<ApprovalRequest>,
    session_approvals: Arc<RwLock<HashMap<String, bool>>>,
    tool_approvals: Arc<RwLock<HashMap<String, bool>>>,
    auto_approve_mode: Arc<RwLock<bool>>,
}

impl ApprovalManager {
    pub fn new() -> (Self, mpsc::Receiver<ApprovalRequest>) {
        let (request_tx, request_rx) = mpsc::channel(32);
        (
            Self {
                pending: Arc::new(RwLock::new(HashMap::new())),
                request_tx,
                session_approvals: Arc::new(RwLock::new(HashMap::new())),
                tool_approvals: Arc::new(RwLock::new(HashMap::new())),
                auto_approve_mode: Arc::new(RwLock::new(false)),
            },
            request_rx,
        )
    }

    pub async fn request_approval(&self, request: ApprovalRequest) -> ApprovalResponse {
        if *self.auto_approve_mode.read().await {
            return ApprovalResponse::Approved;
        }

        let tool_key = &request.tool_name;
        if let Some(&approved) = self.tool_approvals.read().await.get(tool_key) {
            if approved {
                return ApprovalResponse::Approved;
            }
        }

        let session_key = format!("{}:{}", request.tool_name, request.id);
        if let Some(&approved) = self.session_approvals.read().await.get(&session_key) {
            if approved {
                return ApprovalResponse::Approved;
            }
        }

        let (response_tx, response_rx) = oneshot::channel();
        let request_id = request.id.clone();

        self.pending
            .write()
            .await
            .insert(request_id.clone(), response_tx);

        if self.request_tx.send(request).await.is_err() {
            self.pending.write().await.remove(&request_id);
            return ApprovalResponse::Denied;
        }

        match response_rx.await {
            Ok(response) => {
                self.process_response(&request_id, &response).await;
                response
            }
            Err(_) => ApprovalResponse::Denied,
        }
    }

    pub async fn respond(&self, request_id: &str, response: ApprovalResponse) -> bool {
        if let Some(tx) = self.pending.write().await.remove(request_id) {
            tx.send(response).is_ok()
        } else {
            false
        }
    }

    async fn process_response(&self, _request_id: &str, response: &ApprovalResponse) {
        match response {
            ApprovalResponse::ApprovedForSession => {
                self.session_approvals
                    .write()
                    .await
                    .insert(_request_id.to_string(), true);
            }
            ApprovalResponse::ApprovedForTool => {
                let tool_name = _request_id.split(':').next().unwrap_or(_request_id);
                self.tool_approvals
                    .write()
                    .await
                    .insert(tool_name.to_string(), true);
            }
            _ => {}
        }
    }

    pub async fn set_auto_approve(&self, enabled: bool) {
        *self.auto_approve_mode.write().await = enabled;
    }

    pub async fn is_auto_approve(&self) -> bool {
        *self.auto_approve_mode.read().await
    }

    pub async fn approve_tool(&self, tool_name: &str) {
        self.tool_approvals
            .write()
            .await
            .insert(tool_name.to_string(), true);
    }

    pub async fn revoke_tool_approval(&self, tool_name: &str) {
        self.tool_approvals.write().await.remove(tool_name);
    }

    pub async fn clear_session_approvals(&self) {
        self.session_approvals.write().await.clear();
    }

    pub async fn clear_all_approvals(&self) {
        self.session_approvals.write().await.clear();
        self.tool_approvals.write().await.clear();
    }
}

impl Default for ApprovalManager {
    fn default() -> Self {
        Self::new().0
    }
}

pub struct ApprovalPolicy {
    always_require: Vec<String>,
    never_require: Vec<String>,
    path_patterns: Vec<PathPattern>,
    command_patterns: Vec<CommandPattern>,
}

#[derive(Debug, Clone)]
struct PathPattern {
    pattern: String,
    require_approval: bool,
}

#[derive(Debug, Clone)]
struct CommandPattern {
    pattern: String,
    require_approval: bool,
}

impl ApprovalPolicy {
    pub fn new() -> Self {
        Self {
            always_require: vec![
                "execute_command".to_string(),
                "shell".to_string(),
                "write_file".to_string(),
                "apply_patch".to_string(),
            ],
            never_require: vec![
                "read_file".to_string(),
                "list_files".to_string(),
                "search_files".to_string(),
            ],
            path_patterns: vec![
                PathPattern {
                    pattern: "/etc/".to_string(),
                    require_approval: true,
                },
                PathPattern {
                    pattern: "~/.ssh/".to_string(),
                    require_approval: true,
                },
                PathPattern {
                    pattern: ".env".to_string(),
                    require_approval: true,
                },
            ],
            command_patterns: vec![
                CommandPattern {
                    pattern: "rm -rf".to_string(),
                    require_approval: true,
                },
                CommandPattern {
                    pattern: "sudo".to_string(),
                    require_approval: true,
                },
                CommandPattern {
                    pattern: "chmod".to_string(),
                    require_approval: true,
                },
            ],
        }
    }

    pub fn requires_approval(&self, tool_name: &str, input: &serde_json::Value) -> bool {
        if self.never_require.contains(&tool_name.to_string()) {
            return false;
        }

        if self.always_require.contains(&tool_name.to_string()) {
            return true;
        }

        if let Some(path) = input.get("path").and_then(|v| v.as_str()) {
            for pattern in &self.path_patterns {
                if path.contains(&pattern.pattern) && pattern.require_approval {
                    return true;
                }
            }
        }

        if let Some(cmd) = input.get("command").and_then(|v| v.as_str()) {
            for pattern in &self.command_patterns {
                if cmd.contains(&pattern.pattern) && pattern.require_approval {
                    return true;
                }
            }
        }

        false
    }

    pub fn add_always_require(&mut self, tool: impl Into<String>) {
        self.always_require.push(tool.into());
    }

    pub fn add_never_require(&mut self, tool: impl Into<String>) {
        self.never_require.push(tool.into());
    }

    pub fn add_path_pattern(&mut self, pattern: impl Into<String>, require: bool) {
        self.path_patterns.push(PathPattern {
            pattern: pattern.into(),
            require_approval: require,
        });
    }

    pub fn add_command_pattern(&mut self, pattern: impl Into<String>, require: bool) {
        self.command_patterns.push(CommandPattern {
            pattern: pattern.into(),
            require_approval: require,
        });
    }
}

impl Default for ApprovalPolicy {
    fn default() -> Self {
        Self::new()
    }
}
