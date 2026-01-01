#![deny(clippy::all)]
#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffView {
    pub files: Vec<FileDiffView>,
    pub summary: DiffSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileDiffView {
    pub path: String,
    pub original_path: Option<String>,
    pub operation: DiffOperation,
    pub hunks: Vec<DiffHunk>,
    pub is_binary: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiffOperation {
    Add,
    Modify,
    Delete,
    Rename,
    Copy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffHunk {
    pub old_start: usize,
    pub old_lines: usize,
    pub new_start: usize,
    pub new_lines: usize,
    pub lines: Vec<DiffLine>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffLine {
    pub line_type: LineType,
    pub content: String,
    pub old_number: Option<usize>,
    pub new_number: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LineType {
    Context,
    Add,
    Delete,
    Header,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DiffSummary {
    pub files_changed: usize,
    pub insertions: usize,
    pub deletions: usize,
    pub files_added: usize,
    pub files_deleted: usize,
    pub files_modified: usize,
    pub files_renamed: usize,
}

pub struct DiffParser;

impl DiffParser {
    pub fn parse_unified(diff: &str) -> DiffView {
        let mut files = Vec::new();
        let mut current_file: Option<FileDiffView> = None;
        let mut current_hunk: Option<DiffHunk> = None;
        let mut old_line = 0;
        let mut new_line = 0;

        for line in diff.lines() {
            if line.starts_with("diff --git") {
                if let Some(mut file) = current_file.take() {
                    if let Some(hunk) = current_hunk.take() {
                        file.hunks.push(hunk);
                    }
                    files.push(file);
                }
                current_file = Some(FileDiffView {
                    path: String::new(),
                    original_path: None,
                    operation: DiffOperation::Modify,
                    hunks: Vec::new(),
                    is_binary: false,
                });
            } else if line.starts_with("--- ") {
                if let Some(ref mut file) = current_file {
                    let path = line[4..].trim_start_matches("a/").to_string();
                    if path != "/dev/null" {
                        file.original_path = Some(path);
                    }
                }
            } else if line.starts_with("+++ ") {
                if let Some(ref mut file) = current_file {
                    let path = line[4..].trim_start_matches("b/").to_string();
                    if path != "/dev/null" {
                        file.path = path;
                    } else {
                        file.operation = DiffOperation::Delete;
                    }
                    if file.original_path.is_none() {
                        file.operation = DiffOperation::Add;
                    }
                }
            } else if line.starts_with("@@ ") {
                if let Some(ref mut file) = current_file {
                    if let Some(hunk) = current_hunk.take() {
                        file.hunks.push(hunk);
                    }
                }

                if let Some((old, new)) = parse_hunk_header(line) {
                    old_line = old.0;
                    new_line = new.0;
                    current_hunk = Some(DiffHunk {
                        old_start: old.0,
                        old_lines: old.1,
                        new_start: new.0,
                        new_lines: new.1,
                        lines: vec![DiffLine {
                            line_type: LineType::Header,
                            content: line.to_string(),
                            old_number: None,
                            new_number: None,
                        }],
                    });
                }
            } else if let Some(ref mut hunk) = current_hunk {
                if line.starts_with('+') && !line.starts_with("+++") {
                    hunk.lines.push(DiffLine {
                        line_type: LineType::Add,
                        content: line[1..].to_string(),
                        old_number: None,
                        new_number: Some(new_line),
                    });
                    new_line += 1;
                } else if line.starts_with('-') && !line.starts_with("---") {
                    hunk.lines.push(DiffLine {
                        line_type: LineType::Delete,
                        content: line[1..].to_string(),
                        old_number: Some(old_line),
                        new_number: None,
                    });
                    old_line += 1;
                } else if line.starts_with(' ') || line.is_empty() {
                    let content = if line.is_empty() {
                        String::new()
                    } else {
                        line[1..].to_string()
                    };
                    hunk.lines.push(DiffLine {
                        line_type: LineType::Context,
                        content,
                        old_number: Some(old_line),
                        new_number: Some(new_line),
                    });
                    old_line += 1;
                    new_line += 1;
                }
            } else if line.starts_with("Binary files") {
                if let Some(ref mut file) = current_file {
                    file.is_binary = true;
                }
            } else if line.starts_with("rename from ") {
                if let Some(ref mut file) = current_file {
                    file.operation = DiffOperation::Rename;
                    file.original_path = Some(line[12..].to_string());
                }
            } else if line.starts_with("rename to ") {
                if let Some(ref mut file) = current_file {
                    file.path = line[10..].to_string();
                }
            }
        }

        if let Some(mut file) = current_file {
            if let Some(hunk) = current_hunk {
                file.hunks.push(hunk);
            }
            files.push(file);
        }

        let summary = compute_summary(&files);

        DiffView { files, summary }
    }

    pub fn format_for_display(view: &DiffView) -> String {
        let mut output = String::new();

        output.push_str(&format!(
            "{} file(s) changed, {} insertions(+), {} deletions(-)\n\n",
            view.summary.files_changed, view.summary.insertions, view.summary.deletions
        ));

        for file in &view.files {
            let op = match file.operation {
                DiffOperation::Add => "[new file]",
                DiffOperation::Delete => "[deleted]",
                DiffOperation::Modify => "[modified]",
                DiffOperation::Rename => "[renamed]",
                DiffOperation::Copy => "[copied]",
            };

            if let Some(orig) = &file.original_path {
                output.push_str(&format!("{} {} -> {}\n", op, orig, file.path));
            } else {
                output.push_str(&format!("{} {}\n", op, file.path));
            }

            if file.is_binary {
                output.push_str("  (binary file)\n");
                continue;
            }

            for hunk in &file.hunks {
                for line in &hunk.lines {
                    match line.line_type {
                        LineType::Header => {
                            output.push_str(&format!("  {}\n", line.content));
                        }
                        LineType::Add => {
                            output.push_str(&format!("  + {}\n", line.content));
                        }
                        LineType::Delete => {
                            output.push_str(&format!("  - {}\n", line.content));
                        }
                        LineType::Context => {
                            output.push_str(&format!("    {}\n", line.content));
                        }
                    }
                }
            }
            output.push('\n');
        }

        output
    }

    pub fn create_from_changes(changes: &[FileChange]) -> DiffView {
        let mut files = Vec::new();

        for change in changes {
            let hunks = create_hunks(&change.old_content, &change.new_content);
            files.push(FileDiffView {
                path: change.path.clone(),
                original_path: change.original_path.clone(),
                operation: change.operation,
                hunks,
                is_binary: false,
            });
        }

        let summary = compute_summary(&files);
        DiffView { files, summary }
    }
}

#[derive(Debug, Clone)]
pub struct FileChange {
    pub path: String,
    pub original_path: Option<String>,
    pub operation: DiffOperation,
    pub old_content: String,
    pub new_content: String,
}

fn parse_hunk_header(line: &str) -> Option<((usize, usize), (usize, usize))> {
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() < 3 {
        return None;
    }

    let old_range = parts[1].trim_start_matches('-');
    let new_range = parts[2].trim_start_matches('+');

    let old = parse_range(old_range)?;
    let new = parse_range(new_range)?;

    Some((old, new))
}

fn parse_range(s: &str) -> Option<(usize, usize)> {
    let parts: Vec<&str> = s.split(',').collect();
    let start: usize = parts.first()?.parse().ok()?;
    let count: usize = if parts.len() > 1 {
        parts[1].parse().ok()?
    } else {
        1
    };
    Some((start, count))
}

fn compute_summary(files: &[FileDiffView]) -> DiffSummary {
    let mut summary = DiffSummary::default();
    summary.files_changed = files.len();

    for file in files {
        match file.operation {
            DiffOperation::Add => summary.files_added += 1,
            DiffOperation::Delete => summary.files_deleted += 1,
            DiffOperation::Modify => summary.files_modified += 1,
            DiffOperation::Rename | DiffOperation::Copy => summary.files_renamed += 1,
        }

        for hunk in &file.hunks {
            for line in &hunk.lines {
                match line.line_type {
                    LineType::Add => summary.insertions += 1,
                    LineType::Delete => summary.deletions += 1,
                    _ => {}
                }
            }
        }
    }

    summary
}

fn create_hunks(old: &str, new: &str) -> Vec<DiffHunk> {
    let old_lines: Vec<&str> = old.lines().collect();
    let new_lines: Vec<&str> = new.lines().collect();

    let mut hunks = Vec::new();
    let mut hunk_lines = Vec::new();
    let mut old_idx = 0;
    let mut new_idx = 0;
    let mut hunk_old_start = 1;
    let mut hunk_new_start = 1;
    let mut in_hunk = false;

    while old_idx < old_lines.len() || new_idx < new_lines.len() {
        let old_line = old_lines.get(old_idx);
        let new_line = new_lines.get(new_idx);

        match (old_line, new_line) {
            (Some(o), Some(n)) if o == n => {
                if in_hunk {
                    hunk_lines.push(DiffLine {
                        line_type: LineType::Context,
                        content: (*o).to_string(),
                        old_number: Some(old_idx + 1),
                        new_number: Some(new_idx + 1),
                    });
                }
                old_idx += 1;
                new_idx += 1;
            }
            _ => {
                if !in_hunk {
                    in_hunk = true;
                    hunk_old_start = old_idx + 1;
                    hunk_new_start = new_idx + 1;
                }

                if old_line.is_some() && (new_line.is_none() || old_line != new_line) {
                    hunk_lines.push(DiffLine {
                        line_type: LineType::Delete,
                        content: old_line.unwrap().to_string(),
                        old_number: Some(old_idx + 1),
                        new_number: None,
                    });
                    old_idx += 1;
                }

                if new_line.is_some() && (old_line.is_none() || old_line != new_line) {
                    hunk_lines.push(DiffLine {
                        line_type: LineType::Add,
                        content: new_line.unwrap().to_string(),
                        old_number: None,
                        new_number: Some(new_idx + 1),
                    });
                    new_idx += 1;
                }
            }
        }
    }

    if !hunk_lines.is_empty() {
        let old_count = hunk_lines
            .iter()
            .filter(|l| matches!(l.line_type, LineType::Delete | LineType::Context))
            .count();
        let new_count = hunk_lines
            .iter()
            .filter(|l| matches!(l.line_type, LineType::Add | LineType::Context))
            .count();

        hunks.push(DiffHunk {
            old_start: hunk_old_start,
            old_lines: old_count,
            new_start: hunk_new_start,
            new_lines: new_count,
            lines: hunk_lines,
        });
    }

    hunks
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingChange {
    pub id: String,
    pub file: FileDiffView,
    pub status: ChangeStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeStatus {
    Pending,
    Approved,
    Rejected,
    Applied,
}

pub struct ChangeManager {
    pending: HashMap<String, PendingChange>,
}

impl ChangeManager {
    pub fn new() -> Self {
        Self {
            pending: HashMap::new(),
        }
    }

    pub fn add_change(&mut self, id: impl Into<String>, file: FileDiffView) {
        let id = id.into();
        self.pending.insert(
            id.clone(),
            PendingChange {
                id,
                file,
                status: ChangeStatus::Pending,
            },
        );
    }

    pub fn approve(&mut self, id: &str) -> bool {
        if let Some(change) = self.pending.get_mut(id) {
            change.status = ChangeStatus::Approved;
            true
        } else {
            false
        }
    }

    pub fn reject(&mut self, id: &str) -> bool {
        if let Some(change) = self.pending.get_mut(id) {
            change.status = ChangeStatus::Rejected;
            true
        } else {
            false
        }
    }

    pub fn mark_applied(&mut self, id: &str) -> bool {
        if let Some(change) = self.pending.get_mut(id) {
            change.status = ChangeStatus::Applied;
            true
        } else {
            false
        }
    }

    pub fn get_pending(&self) -> Vec<&PendingChange> {
        self.pending
            .values()
            .filter(|c| c.status == ChangeStatus::Pending)
            .collect()
    }

    pub fn get_approved(&self) -> Vec<&PendingChange> {
        self.pending
            .values()
            .filter(|c| c.status == ChangeStatus::Approved)
            .collect()
    }

    pub fn clear_applied(&mut self) {
        self.pending
            .retain(|_, c| c.status != ChangeStatus::Applied);
    }
}

impl Default for ChangeManager {
    fn default() -> Self {
        Self::new()
    }
}
