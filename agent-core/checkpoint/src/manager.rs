#![deny(clippy::all)]
#![forbid(unsafe_code)]

use crate::diff::FileDiff;
use crate::snapshot::{CheckpointMeta, FileSnapshot};
use agent_common::{AgentError, AgentResult};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::fs;
use tokio::process::Command;
use uuid::Uuid;

pub struct CheckpointManager {
    workspace: PathBuf,
    shadow_dir: PathBuf,
    checkpoints: Vec<CheckpointMeta>,
    initialized: bool,
}

impl CheckpointManager {
    pub fn new(workspace: impl AsRef<Path>) -> Self {
        let workspace = workspace.as_ref().to_path_buf();
        let shadow_dir = workspace.join(".agent-checkpoints");

        Self {
            workspace,
            shadow_dir,
            checkpoints: Vec::new(),
            initialized: false,
        }
    }

    pub async fn init(&mut self) -> AgentResult<()> {
        if self.initialized {
            return Ok(());
        }

        fs::create_dir_all(&self.shadow_dir)
            .await
            .map_err(|e| AgentError::io("create shadow dir", e))?;

        let git_dir = self.shadow_dir.join("repo");
        if !git_dir.exists() {
            self.run_git(&["init", "--bare", "repo"]).await?;
        }

        self.load_checkpoints().await?;
        self.initialized = true;
        Ok(())
    }

    pub async fn create(&mut self, message: impl Into<String>) -> AgentResult<CheckpointMeta> {
        self.ensure_initialized()?;

        let message = message.into();
        let id = Uuid::new_v4().to_string();
        let parent_id = self.checkpoints.last().map(|c| c.id.clone());

        let (files, total_size) = self.snapshot_workspace().await?;
        let commit_hash = self.commit_snapshot(&id, &message, &files).await?;

        let meta = CheckpointMeta::new(&id, &message, commit_hash)
            .with_stats(files.len(), total_size);

        let meta = if let Some(parent) = parent_id {
            meta.with_parent(parent)
        } else {
            meta
        };

        self.save_checkpoint_meta(&meta).await?;
        self.checkpoints.push(meta.clone());

        Ok(meta)
    }

    pub async fn create_labeled(
        &mut self,
        label: impl Into<String>,
        message: impl Into<String>,
    ) -> AgentResult<CheckpointMeta> {
        let mut meta = self.create(message).await?;
        meta.label = Some(label.into());
        self.update_checkpoint_meta(&meta).await?;
        Ok(meta)
    }

    pub async fn restore(&self, checkpoint_id: &str) -> AgentResult<()> {
        self.ensure_initialized()?;

        let meta = self
            .checkpoints
            .iter()
            .find(|c| c.id == checkpoint_id)
            .ok_or_else(|| AgentError::not_found(format!("checkpoint: {}", checkpoint_id)))?
            .clone();

        self.run_git(&["checkout", &meta.commit_hash, "--", "."])
            .await?;

        Ok(())
    }

    pub async fn diff_from(&self, checkpoint_id: &str) -> AgentResult<Vec<FileDiff>> {
        self.ensure_initialized()?;

        let meta = self
            .checkpoints
            .iter()
            .find(|c| c.id == checkpoint_id)
            .ok_or_else(|| AgentError::not_found(format!("checkpoint: {}", checkpoint_id)))?
            .clone();

        let output = self
            .run_git_output(&["diff", "--name-status", &meta.commit_hash, "HEAD"])
            .await?;

        self.parse_diff_output(&output).await
    }

    pub async fn diff_between(&self, from_id: &str, to_id: &str) -> AgentResult<Vec<FileDiff>> {
        self.ensure_initialized()?;

        let from_meta = self
            .checkpoints
            .iter()
            .find(|c| c.id == from_id)
            .ok_or_else(|| AgentError::not_found(format!("checkpoint: {}", from_id)))?
            .clone();

        let to_meta = self
            .checkpoints
            .iter()
            .find(|c| c.id == to_id)
            .ok_or_else(|| AgentError::not_found(format!("checkpoint: {}", to_id)))?
            .clone();

        let output = self
            .run_git_output(&[
                "diff",
                "--name-status",
                &from_meta.commit_hash,
                &to_meta.commit_hash,
            ])
            .await?;

        self.parse_diff_output(&output).await
    }

    pub async fn list(&self) -> Vec<CheckpointMeta> {
        self.checkpoints.clone()
    }

    pub async fn get(&self, id: &str) -> Option<CheckpointMeta> {
        self.checkpoints.iter().find(|c| c.id == id).cloned()
    }

    pub async fn get_by_label(&self, label: &str) -> Option<CheckpointMeta> {
        self.checkpoints
            .iter()
            .find(|c| c.label.as_deref() == Some(label))
            .cloned()
    }

    pub async fn delete(&mut self, checkpoint_id: &str) -> AgentResult<()> {
        self.ensure_initialized()?;
        self.checkpoints.retain(|c| c.id != checkpoint_id);

        let meta_path = self.shadow_dir.join("meta").join(format!("{}.json", checkpoint_id));
        if meta_path.exists() {
            fs::remove_file(meta_path)
                .await
                .map_err(|e| AgentError::io("delete checkpoint meta", e))?;
        }

        Ok(())
    }

    pub async fn cleanup_old(&mut self, keep_count: usize) -> AgentResult<usize> {
        self.ensure_initialized()?;

        if self.checkpoints.len() <= keep_count {
            return Ok(0);
        }

        let to_remove = self.checkpoints.len() - keep_count;
        let removed_ids: Vec<_> = self
            .checkpoints
            .iter()
            .take(to_remove)
            .map(|c| c.id.clone())
            .collect();

        for id in &removed_ids {
            self.delete(id).await?;
        }

        Ok(removed_ids.len())
    }

    fn ensure_initialized(&self) -> AgentResult<()> {
        if !self.initialized {
            return Err(AgentError::validation(
                "CheckpointManager not initialized. Call init() first.",
            ));
        }
        Ok(())
    }

    async fn run_git(&self, args: &[&str]) -> AgentResult<()> {
        let status = Command::new("git")
            .args(args)
            .current_dir(&self.shadow_dir)
            .status()
            .await
            .map_err(|e| AgentError::io("run git", e))?;

        if !status.success() {
            return Err(AgentError::internal(format!(
                "Git command failed: git {}",
                args.join(" ")
            )));
        }
        Ok(())
    }

    async fn run_git_output(&self, args: &[&str]) -> AgentResult<String> {
        let output = Command::new("git")
            .args(args)
            .current_dir(&self.shadow_dir)
            .output()
            .await
            .map_err(|e| AgentError::io("run git", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(AgentError::internal(format!("Git error: {}", stderr)));
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    async fn snapshot_workspace(&self) -> AgentResult<(HashMap<String, FileSnapshot>, u64)> {
        let mut files = HashMap::new();
        let mut total_size = 0u64;

        self.collect_files(&self.workspace, &mut files, &mut total_size)
            .await?;

        Ok((files, total_size))
    }

    async fn collect_files(
        &self,
        dir: &Path,
        files: &mut HashMap<String, FileSnapshot>,
        total_size: &mut u64,
    ) -> AgentResult<()> {
        let mut entries = fs::read_dir(dir)
            .await
            .map_err(|e| AgentError::io("read dir", e))?;

        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| AgentError::io("read entry", e))?
        {
            let path = entry.path();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

            if self.should_ignore(name) {
                continue;
            }

            let meta = entry
                .metadata()
                .await
                .map_err(|e| AgentError::io("read metadata", e))?;

            if meta.is_file() {
                let rel_path = path
                    .strip_prefix(&self.workspace)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .to_string();

                let size = meta.len();
                *total_size += size;

                let hash = self.hash_file(&path).await.unwrap_or_default();
                files.insert(rel_path.clone(), FileSnapshot::new(rel_path, hash, size));
            } else if meta.is_dir() {
                Box::pin(self.collect_files(&path, files, total_size)).await?;
            }
        }

        Ok(())
    }

    fn should_ignore(&self, name: &str) -> bool {
        matches!(
            name,
            ".git"
                | ".agent-checkpoints"
                | "node_modules"
                | "target"
                | ".next"
                | "dist"
                | "build"
                | "__pycache__"
                | ".venv"
                | "venv"
        )
    }

    async fn hash_file(&self, path: &Path) -> AgentResult<String> {
        let content = fs::read(path)
            .await
            .map_err(|e| AgentError::io("read file", e))?;

        Ok(format!("{:x}", md5_hash(&content)))
    }

    async fn commit_snapshot(
        &self,
        _id: &str,
        message: &str,
        _files: &HashMap<String, FileSnapshot>,
    ) -> AgentResult<String> {
        self.run_git(&["add", "-A"]).await?;
        self.run_git(&["commit", "-m", message, "--allow-empty"])
            .await?;

        let output = self.run_git_output(&["rev-parse", "HEAD"]).await?;
        Ok(output.trim().to_string())
    }

    async fn load_checkpoints(&mut self) -> AgentResult<()> {
        let meta_dir = self.shadow_dir.join("meta");
        if !meta_dir.exists() {
            fs::create_dir_all(&meta_dir)
                .await
                .map_err(|e| AgentError::io("create meta dir", e))?;
            return Ok(());
        }

        let mut entries = fs::read_dir(&meta_dir)
            .await
            .map_err(|e| AgentError::io("read meta dir", e))?;

        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| AgentError::io("read entry", e))?
        {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("json") {
                if let Ok(content) = fs::read_to_string(&path).await {
                    if let Ok(meta) = serde_json::from_str::<CheckpointMeta>(&content) {
                        self.checkpoints.push(meta);
                    }
                }
            }
        }

        self.checkpoints.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        Ok(())
    }

    async fn save_checkpoint_meta(&self, meta: &CheckpointMeta) -> AgentResult<()> {
        let meta_dir = self.shadow_dir.join("meta");
        fs::create_dir_all(&meta_dir)
            .await
            .map_err(|e| AgentError::io("create meta dir", e))?;

        let path = meta_dir.join(format!("{}.json", meta.id));
        let content = serde_json::to_string_pretty(meta)
            .map_err(|e| AgentError::internal(format!("serialize checkpoint: {}", e)))?;

        fs::write(path, content)
            .await
            .map_err(|e| AgentError::io("write checkpoint meta", e))?;

        Ok(())
    }

    async fn update_checkpoint_meta(&self, meta: &CheckpointMeta) -> AgentResult<()> {
        self.save_checkpoint_meta(meta).await
    }

    async fn parse_diff_output(&self, output: &str) -> AgentResult<Vec<FileDiff>> {
        let mut diffs = Vec::new();

        for line in output.lines() {
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() < 2 {
                continue;
            }

            let status = parts[0];
            let path = parts[1];

            let diff = match status.chars().next() {
                Some('A') => FileDiff::added(path),
                Some('M') => FileDiff::modified(path),
                Some('D') => FileDiff::deleted(path),
                Some('R') => {
                    if parts.len() >= 3 {
                        FileDiff::renamed(parts[1], parts[2])
                    } else {
                        continue;
                    }
                }
                _ => continue,
            };

            diffs.push(diff);
        }

        Ok(diffs)
    }
}

fn md5_hash(data: &[u8]) -> u128 {
    let mut hash: u128 = 0;
    for (i, &byte) in data.iter().enumerate() {
        hash = hash.wrapping_add((byte as u128).wrapping_mul((i as u128).wrapping_add(1)));
        hash = hash.rotate_left(7);
    }
    hash
}
