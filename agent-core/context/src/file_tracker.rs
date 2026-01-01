#![deny(clippy::all)]
#![forbid(unsafe_code)]

use agent_common::AgentResult;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use tokio::fs;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackedFile {
    pub path: PathBuf,
    pub last_modified: Option<SystemTime>,
    pub content_hash: String,
    pub size: u64,
    pub is_modified: bool,
    pub modifications: Vec<FileModification>,
    pub read_count: u32,
    pub write_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileModification {
    pub timestamp: SystemTime,
    pub modification_type: ModificationType,
    pub lines_added: usize,
    pub lines_removed: usize,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModificationType {
    Created,
    Modified,
    Deleted,
    Renamed,
    Read,
}

pub struct FileChangeTracker {
    workspace: PathBuf,
    tracked_files: HashMap<PathBuf, TrackedFile>,
    modified_files: HashSet<PathBuf>,
    created_files: HashSet<PathBuf>,
    deleted_files: HashSet<PathBuf>,
    focus_chain: Vec<PathBuf>,
    max_focus_chain: usize,
}

impl FileChangeTracker {
    pub fn new(workspace: impl AsRef<Path>) -> Self {
        Self {
            workspace: workspace.as_ref().to_path_buf(),
            tracked_files: HashMap::new(),
            modified_files: HashSet::new(),
            created_files: HashSet::new(),
            deleted_files: HashSet::new(),
            focus_chain: Vec::new(),
            max_focus_chain: 20,
        }
    }

    pub fn with_max_focus(mut self, max: usize) -> Self {
        self.max_focus_chain = max;
        self
    }

    pub async fn track_read(&mut self, path: impl AsRef<Path>) -> AgentResult<()> {
        let path = self.normalize_path(path.as_ref());

        if let Some(tracked) = self.tracked_files.get_mut(&path) {
            tracked.read_count += 1;
            self.update_focus_chain(&path);
            return Ok(());
        }

        let tracked = self.create_tracked_file(&path).await?;
        self.tracked_files.insert(path.clone(), tracked);
        self.update_focus_chain(&path);

        Ok(())
    }

    pub async fn track_write(
        &mut self,
        path: impl AsRef<Path>,
        content: &str,
        description: Option<String>,
    ) -> AgentResult<()> {
        let path = self.normalize_path(path.as_ref());
        let new_hash = self.hash_content(content);
        let is_new = !self.tracked_files.contains_key(&path);

        if is_new {
            self.created_files.insert(path.clone());
        }

        let modification = FileModification {
            timestamp: SystemTime::now(),
            modification_type: if is_new {
                ModificationType::Created
            } else {
                ModificationType::Modified
            },
            lines_added: content.lines().count(),
            lines_removed: 0,
            description,
        };

        if let Some(tracked) = self.tracked_files.get_mut(&path) {
            let old_lines = tracked.size as usize / 40;
            let new_lines = content.lines().count();
            let mut mod_entry = modification;
            mod_entry.lines_removed = old_lines.saturating_sub(new_lines);
            mod_entry.lines_added = new_lines.saturating_sub(old_lines);

            tracked.content_hash = new_hash;
            tracked.size = content.len() as u64;
            tracked.is_modified = true;
            tracked.write_count += 1;
            tracked.modifications.push(mod_entry);
            tracked.last_modified = Some(SystemTime::now());
        } else {
            let tracked = TrackedFile {
                path: path.clone(),
                last_modified: Some(SystemTime::now()),
                content_hash: new_hash,
                size: content.len() as u64,
                is_modified: true,
                modifications: vec![modification],
                read_count: 0,
                write_count: 1,
            };
            self.tracked_files.insert(path.clone(), tracked);
        }

        self.modified_files.insert(path.clone());
        self.update_focus_chain(&path);

        Ok(())
    }

    pub fn track_delete(&mut self, path: impl AsRef<Path>) {
        let path = self.normalize_path(path.as_ref());
        self.deleted_files.insert(path.clone());
        self.modified_files.remove(&path);
        self.created_files.remove(&path);

        if let Some(tracked) = self.tracked_files.get_mut(&path) {
            tracked.is_modified = true;
            tracked.modifications.push(FileModification {
                timestamp: SystemTime::now(),
                modification_type: ModificationType::Deleted,
                lines_added: 0,
                lines_removed: 0,
                description: None,
            });
        }
    }

    pub fn get_modified_files(&self) -> Vec<&TrackedFile> {
        self.tracked_files
            .values()
            .filter(|f| f.is_modified)
            .collect()
    }

    pub fn get_created_files(&self) -> Vec<&PathBuf> {
        self.created_files.iter().collect()
    }

    pub fn get_deleted_files(&self) -> Vec<&PathBuf> {
        self.deleted_files.iter().collect()
    }

    pub fn get_focus_chain(&self) -> &[PathBuf] {
        &self.focus_chain
    }

    pub fn get_current_focus(&self) -> Option<&PathBuf> {
        self.focus_chain.last()
    }

    pub fn get_file(&self, path: impl AsRef<Path>) -> Option<&TrackedFile> {
        let path = self.normalize_path(path.as_ref());
        self.tracked_files.get(&path)
    }

    pub fn modification_summary(&self) -> ModificationSummary {
        let mut total_lines_added = 0;
        let mut total_lines_removed = 0;

        for tracked in self.tracked_files.values() {
            for modification in &tracked.modifications {
                total_lines_added += modification.lines_added;
                total_lines_removed += modification.lines_removed;
            }
        }

        ModificationSummary {
            files_read: self.tracked_files.values().filter(|f| f.read_count > 0).count(),
            files_modified: self.modified_files.len(),
            files_created: self.created_files.len(),
            files_deleted: self.deleted_files.len(),
            total_lines_added,
            total_lines_removed,
        }
    }

    pub fn clear_modifications(&mut self) {
        self.modified_files.clear();
        self.created_files.clear();
        self.deleted_files.clear();

        for tracked in self.tracked_files.values_mut() {
            tracked.is_modified = false;
            tracked.modifications.clear();
        }
    }

    fn normalize_path(&self, path: &Path) -> PathBuf {
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.workspace.join(path)
        }
    }

    fn update_focus_chain(&mut self, path: &PathBuf) {
        self.focus_chain.retain(|p| p != path);
        self.focus_chain.push(path.clone());

        if self.focus_chain.len() > self.max_focus_chain {
            self.focus_chain.remove(0);
        }
    }

    async fn create_tracked_file(&self, path: &PathBuf) -> AgentResult<TrackedFile> {
        let metadata = fs::metadata(path).await.ok();

        let (size, last_modified) = if let Some(meta) = metadata {
            (meta.len(), meta.modified().ok())
        } else {
            (0, None)
        };

        let content_hash = if path.exists() {
            if let Ok(content) = fs::read_to_string(path).await {
                self.hash_content(&content)
            } else {
                String::new()
            }
        } else {
            String::new()
        };

        Ok(TrackedFile {
            path: path.clone(),
            last_modified,
            content_hash,
            size,
            is_modified: false,
            modifications: Vec::new(),
            read_count: 1,
            write_count: 0,
        })
    }

    fn hash_content(&self, content: &str) -> String {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        content.hash(&mut hasher);
        format!("{:x}", hasher.finish())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModificationSummary {
    pub files_read: usize,
    pub files_modified: usize,
    pub files_created: usize,
    pub files_deleted: usize,
    pub total_lines_added: usize,
    pub total_lines_removed: usize,
}

impl ModificationSummary {
    pub fn to_string_summary(&self) -> String {
        format!(
            "{} files read, {} modified, {} created, {} deleted (+{} -{} lines)",
            self.files_read,
            self.files_modified,
            self.files_created,
            self.files_deleted,
            self.total_lines_added,
            self.total_lines_removed
        )
    }
}

impl Default for FileChangeTracker {
    fn default() -> Self {
        Self::new(".")
    }
}
