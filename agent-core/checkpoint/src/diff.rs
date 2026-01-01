#![deny(clippy::all)]
#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileDiff {
    pub path: String,
    pub old_path: Option<String>,
    pub status: DiffStatus,
    pub hunks: Vec<DiffHunk>,
    pub binary: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiffStatus {
    Added,
    Modified,
    Deleted,
    Renamed,
    Copied,
    Unchanged,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffHunk {
    pub old_start: usize,
    pub old_count: usize,
    pub new_start: usize,
    pub new_count: usize,
    pub lines: Vec<DiffLine>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffLine {
    pub kind: LineKind,
    pub content: String,
    pub old_line: Option<usize>,
    pub new_line: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LineKind {
    Context,
    Addition,
    Deletion,
}

impl FileDiff {
    pub fn new(path: impl Into<String>, status: DiffStatus) -> Self {
        Self {
            path: path.into(),
            old_path: None,
            status,
            hunks: Vec::new(),
            binary: false,
        }
    }

    pub fn added(path: impl Into<String>) -> Self {
        Self::new(path, DiffStatus::Added)
    }

    pub fn modified(path: impl Into<String>) -> Self {
        Self::new(path, DiffStatus::Modified)
    }

    pub fn deleted(path: impl Into<String>) -> Self {
        Self::new(path, DiffStatus::Deleted)
    }

    pub fn renamed(old_path: impl Into<String>, new_path: impl Into<String>) -> Self {
        let mut diff = Self::new(new_path, DiffStatus::Renamed);
        diff.old_path = Some(old_path.into());
        diff
    }

    pub fn with_hunk(mut self, hunk: DiffHunk) -> Self {
        self.hunks.push(hunk);
        self
    }

    pub fn as_binary(mut self) -> Self {
        self.binary = true;
        self
    }

    pub fn additions(&self) -> usize {
        self.hunks
            .iter()
            .flat_map(|h| &h.lines)
            .filter(|l| l.kind == LineKind::Addition)
            .count()
    }

    pub fn deletions(&self) -> usize {
        self.hunks
            .iter()
            .flat_map(|h| &h.lines)
            .filter(|l| l.kind == LineKind::Deletion)
            .count()
    }
}

impl DiffHunk {
    pub fn new(old_start: usize, old_count: usize, new_start: usize, new_count: usize) -> Self {
        Self {
            old_start,
            old_count,
            new_start,
            new_count,
            lines: Vec::new(),
        }
    }

    pub fn add_context(mut self, line: usize, content: impl Into<String>) -> Self {
        self.lines.push(DiffLine {
            kind: LineKind::Context,
            content: content.into(),
            old_line: Some(line),
            new_line: Some(line),
        });
        self
    }

    pub fn add_addition(mut self, new_line: usize, content: impl Into<String>) -> Self {
        self.lines.push(DiffLine {
            kind: LineKind::Addition,
            content: content.into(),
            old_line: None,
            new_line: Some(new_line),
        });
        self
    }

    pub fn add_deletion(mut self, old_line: usize, content: impl Into<String>) -> Self {
        self.lines.push(DiffLine {
            kind: LineKind::Deletion,
            content: content.into(),
            old_line: Some(old_line),
            new_line: None,
        });
        self
    }
}
