#![deny(clippy::all)]
#![forbid(unsafe_code)]

use agent_common::{AgentError, AgentResult};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Patch {
    pub files: Vec<FilePatch>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilePatch {
    pub old_path: Option<PathBuf>,
    pub new_path: Option<PathBuf>,
    pub operation: PatchOperation,
    pub hunks: Vec<Hunk>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PatchOperation {
    Create,
    Delete,
    Modify,
    Rename,
    Copy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hunk {
    pub old_start: usize,
    pub old_count: usize,
    pub new_start: usize,
    pub new_count: usize,
    pub lines: Vec<PatchLine>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchLine {
    pub kind: LineKind,
    pub content: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LineKind {
    Context,
    Add,
    Delete,
    Header,
}

impl Patch {
    pub fn parse(diff_text: &str) -> AgentResult<Self> {
        let mut files = Vec::new();
        let mut current_file: Option<FilePatch> = None;
        let mut current_hunk: Option<Hunk> = None;

        for line in diff_text.lines() {
            if line.starts_with("diff --git") || line.starts_with("diff ") {
                if let Some(mut file) = current_file.take() {
                    if let Some(hunk) = current_hunk.take() {
                        file.hunks.push(hunk);
                    }
                    files.push(file);
                }

                let paths = parse_diff_header(line);
                current_file = Some(FilePatch {
                    old_path: paths.0,
                    new_path: paths.1,
                    operation: PatchOperation::Modify,
                    hunks: Vec::new(),
                });
                continue;
            }

            if line.starts_with("--- ") {
                if let Some(ref mut file) = current_file {
                    let path = line.trim_start_matches("--- ").trim_start_matches("a/");
                    if path != "/dev/null" {
                        file.old_path = Some(PathBuf::from(path));
                    } else {
                        file.operation = PatchOperation::Create;
                    }
                }
                continue;
            }

            if line.starts_with("+++ ") {
                if let Some(ref mut file) = current_file {
                    let path = line.trim_start_matches("+++ ").trim_start_matches("b/");
                    if path != "/dev/null" {
                        file.new_path = Some(PathBuf::from(path));
                    } else {
                        file.operation = PatchOperation::Delete;
                    }
                }
                continue;
            }

            if line.starts_with("@@ ") {
                if let Some(ref mut file) = current_file {
                    if let Some(hunk) = current_hunk.take() {
                        file.hunks.push(hunk);
                    }
                }

                if let Some(hunk) = parse_hunk_header(line) {
                    current_hunk = Some(hunk);
                }
                continue;
            }

            if line.starts_with("new file mode") {
                if let Some(ref mut file) = current_file {
                    file.operation = PatchOperation::Create;
                }
                continue;
            }

            if line.starts_with("deleted file mode") {
                if let Some(ref mut file) = current_file {
                    file.operation = PatchOperation::Delete;
                }
                continue;
            }

            if line.starts_with("rename from") {
                if let Some(ref mut file) = current_file {
                    file.operation = PatchOperation::Rename;
                    let path = line.trim_start_matches("rename from ");
                    file.old_path = Some(PathBuf::from(path));
                }
                continue;
            }

            if line.starts_with("rename to") {
                if let Some(ref mut file) = current_file {
                    let path = line.trim_start_matches("rename to ");
                    file.new_path = Some(PathBuf::from(path));
                }
                continue;
            }

            if let Some(ref mut hunk) = current_hunk {
                let diff_line = if line.starts_with('+') {
                    PatchLine {
                        kind: LineKind::Add,
                        content: line[1..].to_string(),
                    }
                } else if line.starts_with('-') {
                    PatchLine {
                        kind: LineKind::Delete,
                        content: line[1..].to_string(),
                    }
                } else if line.starts_with(' ') || line.is_empty() {
                    PatchLine {
                        kind: LineKind::Context,
                        content: if line.is_empty() {
                            String::new()
                        } else {
                            line[1..].to_string()
                        },
                    }
                } else {
                    PatchLine {
                        kind: LineKind::Context,
                        content: line.to_string(),
                    }
                };

                hunk.lines.push(diff_line);
            }
        }

        if let Some(mut file) = current_file {
            if let Some(hunk) = current_hunk {
                file.hunks.push(hunk);
            }
            files.push(file);
        }

        Ok(Patch { files })
    }
}

fn parse_diff_header(line: &str) -> (Option<PathBuf>, Option<PathBuf>) {
    let parts: Vec<&str> = line.split_whitespace().collect();

    let old_path = parts.iter().find(|p| p.starts_with("a/")).map(|p| {
        PathBuf::from(p.trim_start_matches("a/"))
    });

    let new_path = parts.iter().find(|p| p.starts_with("b/")).map(|p| {
        PathBuf::from(p.trim_start_matches("b/"))
    });

    (old_path, new_path)
}

fn parse_hunk_header(line: &str) -> Option<Hunk> {
    let line = line.trim_start_matches("@@ ");
    let end = line.find(" @@")?;
    let range_str = &line[..end];

    let parts: Vec<&str> = range_str.split_whitespace().collect();
    if parts.len() < 2 {
        return None;
    }

    let (old_start, old_count) = parse_range(parts[0].trim_start_matches('-'))?;
    let (new_start, new_count) = parse_range(parts[1].trim_start_matches('+'))?;

    Some(Hunk {
        old_start,
        old_count,
        new_start,
        new_count,
        lines: Vec::new(),
    })
}

fn parse_range(s: &str) -> Option<(usize, usize)> {
    let parts: Vec<&str> = s.split(',').collect();
    let start: usize = parts.first()?.parse().ok()?;
    let count: usize = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(1);
    Some((start, count))
}

pub struct PatchApplier {
    fuzz_factor: usize,
    reverse: bool,
    dry_run: bool,
}

impl PatchApplier {
    pub fn new() -> Self {
        Self {
            fuzz_factor: 2,
            reverse: false,
            dry_run: false,
        }
    }

    pub fn with_fuzz(mut self, fuzz: usize) -> Self {
        self.fuzz_factor = fuzz;
        self
    }

    pub fn reverse(mut self) -> Self {
        self.reverse = true;
        self
    }

    pub fn dry_run(mut self) -> Self {
        self.dry_run = true;
        self
    }

    pub fn apply_to_content(&self, content: &str, file_patch: &FilePatch) -> AgentResult<String> {
        let mut lines: Vec<String> = content.lines().map(String::from).collect();

        for hunk in &file_patch.hunks {
            lines = self.apply_hunk(&lines, hunk)?;
        }

        Ok(lines.join("\n"))
    }

    fn apply_hunk(&self, lines: &[String], hunk: &Hunk) -> AgentResult<Vec<String>> {
        let target_line = if self.reverse {
            hunk.new_start
        } else {
            hunk.old_start
        };

        let offset = self.find_hunk_offset(lines, hunk, target_line)?;
        let actual_start = (target_line as isize + offset) as usize;

        let mut result = Vec::new();
        result.extend(lines.iter().take(actual_start.saturating_sub(1)).cloned());

        let mut consumed = 0;
        for diff_line in &hunk.lines {
            match diff_line.kind {
                LineKind::Context => {
                    let line_idx = actual_start - 1 + consumed;
                    if line_idx < lines.len() {
                        result.push(lines[line_idx].clone());
                        consumed += 1;
                    }
                }
                LineKind::Add => {
                    if !self.reverse {
                        result.push(diff_line.content.clone());
                    } else {
                        consumed += 1;
                    }
                }
                LineKind::Delete => {
                    if self.reverse {
                        result.push(diff_line.content.clone());
                    } else {
                        consumed += 1;
                    }
                }
                LineKind::Header => {}
            }
        }

        let skip_count = actual_start - 1 + consumed;
        result.extend(lines.iter().skip(skip_count).cloned());

        Ok(result)
    }

    fn find_hunk_offset(
        &self,
        lines: &[String],
        hunk: &Hunk,
        target_line: usize,
    ) -> AgentResult<isize> {
        let context_lines: Vec<&str> = hunk
            .lines
            .iter()
            .filter(|l| l.kind == LineKind::Context || l.kind == LineKind::Delete)
            .map(|l| l.content.as_str())
            .collect();

        if context_lines.is_empty() {
            return Ok(0);
        }

        for fuzz in 0..=self.fuzz_factor {
            for offset in [0isize, -(fuzz as isize), fuzz as isize] {
                let start = ((target_line as isize + offset) as usize).saturating_sub(1);

                if self.matches_context(lines, start, &context_lines) {
                    return Ok(offset);
                }
            }
        }

        Err(AgentError::validation(format!(
            "could not find matching context at line {} (fuzz={})",
            target_line, self.fuzz_factor
        )))
    }

    fn matches_context(&self, lines: &[String], start: usize, context: &[&str]) -> bool {
        if start + context.len() > lines.len() {
            return false;
        }

        for (i, ctx_line) in context.iter().enumerate() {
            if lines[start + i].trim() != ctx_line.trim() {
                return false;
            }
        }

        true
    }

    pub fn generate_patch(
        old_content: &str,
        new_content: &str,
        path: &str,
    ) -> FilePatch {
        let old_lines: Vec<&str> = old_content.lines().collect();
        let new_lines: Vec<&str> = new_content.lines().collect();

        let mut hunks = Vec::new();
        let mut diff_lines = Vec::new();
        let mut old_idx = 0;
        let mut new_idx = 0;
        let mut hunk_old_start = 1;
        let mut hunk_new_start = 1;

        while old_idx < old_lines.len() || new_idx < new_lines.len() {
            if old_idx < old_lines.len()
                && new_idx < new_lines.len()
                && old_lines[old_idx] == new_lines[new_idx]
            {
                if !diff_lines.is_empty() {
                    diff_lines.push(PatchLine {
                        kind: LineKind::Context,
                        content: old_lines[old_idx].to_string(),
                    });
                }
                old_idx += 1;
                new_idx += 1;
            } else if old_idx < old_lines.len()
                && (new_idx >= new_lines.len()
                    || !new_lines[new_idx..].contains(&old_lines[old_idx]))
            {
                if diff_lines.is_empty() {
                    hunk_old_start = old_idx + 1;
                    hunk_new_start = new_idx + 1;
                }
                diff_lines.push(PatchLine {
                    kind: LineKind::Delete,
                    content: old_lines[old_idx].to_string(),
                });
                old_idx += 1;
            } else if new_idx < new_lines.len() {
                if diff_lines.is_empty() {
                    hunk_old_start = old_idx + 1;
                    hunk_new_start = new_idx + 1;
                }
                diff_lines.push(PatchLine {
                    kind: LineKind::Add,
                    content: new_lines[new_idx].to_string(),
                });
                new_idx += 1;
            }

            let context_run = diff_lines
                .iter()
                .rev()
                .take_while(|l| l.kind == LineKind::Context)
                .count();

            if context_run >= 3 && diff_lines.iter().any(|l| l.kind != LineKind::Context) {
                let old_count = diff_lines
                    .iter()
                    .filter(|l| l.kind == LineKind::Context || l.kind == LineKind::Delete)
                    .count();
                let new_count = diff_lines
                    .iter()
                    .filter(|l| l.kind == LineKind::Context || l.kind == LineKind::Add)
                    .count();

                hunks.push(Hunk {
                    old_start: hunk_old_start,
                    old_count,
                    new_start: hunk_new_start,
                    new_count,
                    lines: std::mem::take(&mut diff_lines),
                });
            }
        }

        if !diff_lines.is_empty() && diff_lines.iter().any(|l| l.kind != LineKind::Context) {
            let old_count = diff_lines
                .iter()
                .filter(|l| l.kind == LineKind::Context || l.kind == LineKind::Delete)
                .count();
            let new_count = diff_lines
                .iter()
                .filter(|l| l.kind == LineKind::Context || l.kind == LineKind::Add)
                .count();

            hunks.push(Hunk {
                old_start: hunk_old_start,
                old_count,
                new_start: hunk_new_start,
                new_count,
                lines: diff_lines,
            });
        }

        let operation = if old_content.is_empty() {
            PatchOperation::Create
        } else if new_content.is_empty() {
            PatchOperation::Delete
        } else {
            PatchOperation::Modify
        };

        FilePatch {
            old_path: Some(PathBuf::from(path)),
            new_path: Some(PathBuf::from(path)),
            operation,
            hunks,
        }
    }
}

impl Default for PatchApplier {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Default)]
pub struct PatchResult {
    pub applied: Vec<PathBuf>,
    pub failed: Vec<(PathBuf, String)>,
    pub skipped: Vec<PathBuf>,
}

impl PatchResult {
    pub fn is_success(&self) -> bool {
        self.failed.is_empty()
    }

    pub fn summary(&self) -> String {
        format!(
            "{} applied, {} failed, {} skipped",
            self.applied.len(),
            self.failed.len(),
            self.skipped.len()
        )
    }
}
