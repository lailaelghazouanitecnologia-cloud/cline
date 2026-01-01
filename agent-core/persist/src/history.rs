#![deny(clippy::all)]
#![forbid(unsafe_code)]

use agent_common::AgentResult;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::storage::StorageManager;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub id: String,
    pub session_id: String,
    pub timestamp: DateTime<Utc>,
    pub role: MessageRole,
    pub content: String,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    User,
    Assistant,
    System,
    Tool,
}

impl HistoryEntry {
    pub fn new(session_id: impl Into<String>, role: MessageRole, content: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            session_id: session_id.into(),
            timestamp: Utc::now(),
            role,
            content: content.into(),
            metadata: None,
        }
    }

    pub fn with_metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = Some(metadata);
        self
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ConversationHistory {
    pub session_id: String,
    pub entries: Vec<HistoryEntry>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl ConversationHistory {
    pub fn new(session_id: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            session_id: session_id.into(),
            entries: Vec::new(),
            created_at: now,
            updated_at: now,
        }
    }

    pub fn add(&mut self, entry: HistoryEntry) {
        self.entries.push(entry);
        self.updated_at = Utc::now();
    }

    pub fn add_user(&mut self, content: impl Into<String>) {
        let entry = HistoryEntry::new(&self.session_id, MessageRole::User, content);
        self.add(entry);
    }

    pub fn add_assistant(&mut self, content: impl Into<String>) {
        let entry = HistoryEntry::new(&self.session_id, MessageRole::Assistant, content);
        self.add(entry);
    }

    pub fn last_n(&self, n: usize) -> Vec<&HistoryEntry> {
        self.entries.iter().rev().take(n).rev().collect()
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.updated_at = Utc::now();
    }
}

pub struct HistoryStore {
    storage: StorageManager,
}

impl HistoryStore {
    pub fn new(storage: StorageManager) -> Self {
        Self { storage: storage.subdir("history") }
    }

    pub async fn save(&self, history: &ConversationHistory) -> AgentResult<()> {
        self.storage.write_json(&history.session_id, history).await
    }

    pub async fn load(&self, session_id: &str) -> AgentResult<Option<ConversationHistory>> {
        self.storage.read_json(session_id).await
    }

    pub async fn delete(&self, session_id: &str) -> AgentResult<()> {
        self.storage.delete(session_id).await
    }

    pub async fn list_sessions(&self) -> AgentResult<Vec<String>> {
        self.storage.list_files("json").await
    }
}

impl Default for HistoryStore {
    fn default() -> Self {
        Self::new(StorageManager::default())
    }
}
