#![deny(clippy::all)]
#![forbid(unsafe_code)]

use agent_common::AgentResult;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::storage::StorageManager;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub id: String,
    pub name: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub working_directory: String,
    pub model_id: String,
    pub message_count: usize,
    pub token_count: u64,
    pub status: SessionStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    Active,
    Paused,
    Completed,
    Archived,
}

impl SessionInfo {
    pub fn new(working_directory: impl Into<String>, model_id: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            name: None,
            created_at: now,
            updated_at: now,
            working_directory: working_directory.into(),
            model_id: model_id.into(),
            message_count: 0,
            token_count: 0,
            status: SessionStatus::Active,
        }
    }

    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    pub fn update_stats(&mut self, messages: usize, tokens: u64) {
        self.message_count = messages;
        self.token_count = tokens;
        self.updated_at = Utc::now();
    }

    pub fn set_status(&mut self, status: SessionStatus) {
        self.status = status;
        self.updated_at = Utc::now();
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionIndex {
    pub sessions: Vec<SessionInfo>,
}

impl SessionIndex {
    pub fn add(&mut self, session: SessionInfo) {
        self.sessions.push(session);
    }

    pub fn remove(&mut self, session_id: &str) {
        self.sessions.retain(|s| s.id != session_id);
    }

    pub fn get(&self, session_id: &str) -> Option<&SessionInfo> {
        self.sessions.iter().find(|s| s.id == session_id)
    }

    pub fn get_mut(&mut self, session_id: &str) -> Option<&mut SessionInfo> {
        self.sessions.iter_mut().find(|s| s.id == session_id)
    }

    pub fn recent(&self, limit: usize) -> Vec<&SessionInfo> {
        let mut sorted: Vec<_> = self.sessions.iter().collect();
        sorted.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        sorted.into_iter().take(limit).collect()
    }

    pub fn active(&self) -> Vec<&SessionInfo> {
        self.sessions
            .iter()
            .filter(|s| s.status == SessionStatus::Active)
            .collect()
    }
}

pub struct SessionStore {
    storage: StorageManager,
}

impl SessionStore {
    pub fn new(storage: StorageManager) -> Self {
        Self { storage: storage.subdir("sessions") }
    }

    pub async fn save_index(&self, index: &SessionIndex) -> AgentResult<()> {
        self.storage.write_json("index", index).await
    }

    pub async fn load_index(&self) -> AgentResult<SessionIndex> {
        self.storage
            .read_json("index")
            .await
            .map(|opt| opt.unwrap_or_default())
    }

    pub async fn create(&self, session: SessionInfo) -> AgentResult<()> {
        let mut index = self.load_index().await?;
        index.add(session);
        self.save_index(&index).await
    }

    pub async fn update(&self, session: SessionInfo) -> AgentResult<()> {
        let mut index = self.load_index().await?;
        if let Some(existing) = index.get_mut(&session.id) {
            *existing = session;
        }
        self.save_index(&index).await
    }

    pub async fn delete(&self, session_id: &str) -> AgentResult<()> {
        let mut index = self.load_index().await?;
        index.remove(session_id);
        self.save_index(&index).await
    }
}

impl Default for SessionStore {
    fn default() -> Self {
        Self::new(StorageManager::default())
    }
}
