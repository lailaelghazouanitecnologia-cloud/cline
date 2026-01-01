#![deny(clippy::all)]
#![forbid(unsafe_code)]

use agent_common::AgentResult;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::history::ConversationHistory;
use crate::session::{SessionInfo, SessionStatus};
use crate::storage::StorageManager;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSnapshot {
    pub session_info: SessionInfo,
    pub history: ConversationHistory,
    pub state: SessionState,
    pub tool_state: ToolState,
    pub context_state: ContextState,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionState {
    pub mode: OperationMode,
    pub pending_tool_calls: Vec<PendingToolCall>,
    pub last_message_id: Option<String>,
    pub turn_count: usize,
    pub awaiting_response: bool,
    pub paused_reason: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationMode {
    #[default]
    Act,
    Plan,
    Paused,
    WaitingApproval,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingToolCall {
    pub id: String,
    pub name: String,
    pub input: serde_json::Value,
    pub status: ToolCallStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolCallStatus {
    Pending,
    AwaitingApproval,
    Approved,
    Rejected,
    Executing,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ToolState {
    pub approved_tools: Vec<String>,
    pub rejected_tools: Vec<String>,
    pub tool_results: Vec<ToolResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub call_id: String,
    pub tool_name: String,
    pub output: serde_json::Value,
    pub is_error: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ContextState {
    pub total_tokens: usize,
    pub message_count: usize,
    pub compacted: bool,
    pub summary: Option<String>,
    pub pinned_messages: Vec<String>,
}

impl SessionSnapshot {
    pub fn new(session_info: SessionInfo, history: ConversationHistory) -> Self {
        Self {
            session_info,
            history,
            state: SessionState::default(),
            tool_state: ToolState::default(),
            context_state: ContextState::default(),
            created_at: Utc::now(),
        }
    }

    pub fn with_state(mut self, state: SessionState) -> Self {
        self.state = state;
        self
    }

    pub fn can_resume(&self) -> bool {
        matches!(
            self.session_info.status,
            SessionStatus::Active | SessionStatus::Paused
        )
    }

    pub fn is_complete(&self) -> bool {
        self.session_info.status == SessionStatus::Completed
    }
}

pub struct ResumeManager {
    storage: StorageManager,
}

impl ResumeManager {
    pub fn new(storage: StorageManager) -> Self {
        Self {
            storage: storage.subdir("snapshots"),
        }
    }

    pub async fn save_snapshot(&self, snapshot: &SessionSnapshot) -> AgentResult<()> {
        self.storage
            .write_json(&snapshot.session_info.id, snapshot)
            .await
    }

    pub async fn load_snapshot(&self, session_id: &str) -> AgentResult<Option<SessionSnapshot>> {
        self.storage.read_json(session_id).await
    }

    pub async fn delete_snapshot(&self, session_id: &str) -> AgentResult<()> {
        self.storage.delete(session_id).await
    }

    pub async fn list_resumable(&self) -> AgentResult<Vec<String>> {
        self.storage.list_files("json").await
    }

    pub async fn get_latest(&self) -> AgentResult<Option<SessionSnapshot>> {
        let sessions = self.list_resumable().await?;

        let mut latest: Option<SessionSnapshot> = None;
        for session_id in sessions {
            if let Some(snapshot) = self.load_snapshot(&session_id).await? {
                if snapshot.can_resume() {
                    match &latest {
                        None => latest = Some(snapshot),
                        Some(current) => {
                            if snapshot.created_at > current.created_at {
                                latest = Some(snapshot);
                            }
                        }
                    }
                }
            }
        }

        Ok(latest)
    }

    pub async fn cleanup_old(&self, max_age_days: u32) -> AgentResult<usize> {
        let sessions = self.list_resumable().await?;
        let mut deleted = 0;
        let cutoff = Utc::now() - chrono::Duration::days(max_age_days as i64);

        for session_id in sessions {
            if let Some(snapshot) = self.load_snapshot(&session_id).await? {
                if snapshot.created_at < cutoff {
                    self.delete_snapshot(&session_id).await?;
                    deleted += 1;
                }
            }
        }

        Ok(deleted)
    }

    pub async fn auto_save(
        &self,
        session_info: &SessionInfo,
        history: &ConversationHistory,
        state: &SessionState,
    ) -> AgentResult<()> {
        let snapshot = SessionSnapshot {
            session_info: session_info.clone(),
            history: history.clone(),
            state: state.clone(),
            tool_state: ToolState::default(),
            context_state: ContextState::default(),
            created_at: Utc::now(),
        };

        self.save_snapshot(&snapshot).await
    }
}

impl Default for ResumeManager {
    fn default() -> Self {
        Self::new(StorageManager::default())
    }
}
