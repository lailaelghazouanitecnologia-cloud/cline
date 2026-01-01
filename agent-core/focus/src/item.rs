#![deny(clippy::all)]
#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use std::time::SystemTime;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FocusStatus {
    Pending,
    InProgress,
    Completed,
    Blocked,
    Skipped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FocusType {
    Task,
    Subtask,
    FileEdit,
    Command,
    Question,
    Checkpoint,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FocusItem {
    pub id: String,
    pub parent_id: Option<String>,
    pub focus_type: FocusType,
    pub title: String,
    pub description: Option<String>,
    pub status: FocusStatus,
    pub priority: u8,
    pub created_at: SystemTime,
    pub updated_at: SystemTime,
    pub metadata: serde_json::Value,
}

impl FocusItem {
    pub fn new(id: impl Into<String>, focus_type: FocusType, title: impl Into<String>) -> Self {
        let now = SystemTime::now();
        Self {
            id: id.into(),
            parent_id: None,
            focus_type,
            title: title.into(),
            description: None,
            status: FocusStatus::Pending,
            priority: 5,
            created_at: now,
            updated_at: now,
            metadata: serde_json::Value::Null,
        }
    }

    pub fn task(id: impl Into<String>, title: impl Into<String>) -> Self {
        Self::new(id, FocusType::Task, title)
    }

    pub fn subtask(id: impl Into<String>, parent: impl Into<String>, title: impl Into<String>) -> Self {
        let mut item = Self::new(id, FocusType::Subtask, title);
        item.parent_id = Some(parent.into());
        item
    }

    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    pub fn with_priority(mut self, priority: u8) -> Self {
        self.priority = priority.min(10);
        self
    }

    pub fn with_parent(mut self, parent_id: impl Into<String>) -> Self {
        self.parent_id = Some(parent_id.into());
        self
    }

    pub fn with_metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = metadata;
        self
    }

    pub fn start(&mut self) {
        self.status = FocusStatus::InProgress;
        self.updated_at = SystemTime::now();
    }

    pub fn complete(&mut self) {
        self.status = FocusStatus::Completed;
        self.updated_at = SystemTime::now();
    }

    pub fn block(&mut self) {
        self.status = FocusStatus::Blocked;
        self.updated_at = SystemTime::now();
    }

    pub fn skip(&mut self) {
        self.status = FocusStatus::Skipped;
        self.updated_at = SystemTime::now();
    }

    pub fn is_active(&self) -> bool {
        matches!(self.status, FocusStatus::Pending | FocusStatus::InProgress)
    }

    pub fn is_done(&self) -> bool {
        matches!(self.status, FocusStatus::Completed | FocusStatus::Skipped)
    }
}
