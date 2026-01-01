#![deny(clippy::all)]
#![forbid(unsafe_code)]

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointMeta {
    pub id: String,
    pub label: Option<String>,
    pub message: String,
    pub created_at: DateTime<Utc>,
    pub parent_id: Option<String>,
    pub commit_hash: String,
    pub file_count: usize,
    pub total_size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    pub meta: CheckpointMeta,
    pub files: HashMap<String, FileSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileSnapshot {
    pub path: String,
    pub content_hash: String,
    pub size: u64,
    pub modified: bool,
    pub created: bool,
    pub deleted: bool,
}

impl CheckpointMeta {
    pub fn new(id: impl Into<String>, message: impl Into<String>, commit_hash: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: None,
            message: message.into(),
            created_at: Utc::now(),
            parent_id: None,
            commit_hash: commit_hash.into(),
            file_count: 0,
            total_size: 0,
        }
    }

    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn with_parent(mut self, parent_id: impl Into<String>) -> Self {
        self.parent_id = Some(parent_id.into());
        self
    }

    pub fn with_stats(mut self, file_count: usize, total_size: u64) -> Self {
        self.file_count = file_count;
        self.total_size = total_size;
        self
    }
}

impl FileSnapshot {
    pub fn new(path: impl Into<String>, content_hash: impl Into<String>, size: u64) -> Self {
        Self {
            path: path.into(),
            content_hash: content_hash.into(),
            size,
            modified: false,
            created: false,
            deleted: false,
        }
    }

    pub fn as_modified(mut self) -> Self {
        self.modified = true;
        self
    }

    pub fn as_created(mut self) -> Self {
        self.created = true;
        self
    }

    pub fn as_deleted(mut self) -> Self {
        self.deleted = true;
        self
    }
}
