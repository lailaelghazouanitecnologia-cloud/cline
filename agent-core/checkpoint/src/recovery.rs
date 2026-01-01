#![deny(clippy::all)]
#![forbid(unsafe_code)]

use crate::manager::CheckpointManager;
use crate::snapshot::CheckpointMeta;
use agent_common::{AgentError, AgentResult};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::fs;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryStrategy {
    LastCheckpoint,
    LastLabeled,
    Specific,
    Interactive,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryState {
    pub session_id: String,
    pub last_checkpoint_id: Option<String>,
    pub pending_operations: Vec<PendingOperation>,
    pub crash_detected: bool,
    pub recovery_timestamp: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingOperation {
    pub id: String,
    pub operation_type: String,
    pub target_path: Option<PathBuf>,
    pub started_at: u64,
    pub completed: bool,
}

pub struct RecoveryManager {
    workspace: PathBuf,
    state_file: PathBuf,
    checkpoint_manager: CheckpointManager,
    strategy: RecoveryStrategy,
    auto_checkpoint_interval: usize,
    operation_counter: usize,
}

impl RecoveryManager {
    pub fn new(workspace: impl AsRef<Path>) -> Self {
        let workspace = workspace.as_ref().to_path_buf();
        let state_file = workspace.join(".agent-recovery-state.json");

        Self {
            workspace: workspace.clone(),
            state_file,
            checkpoint_manager: CheckpointManager::new(&workspace),
            strategy: RecoveryStrategy::LastCheckpoint,
            auto_checkpoint_interval: 10,
            operation_counter: 0,
        }
    }

    pub fn with_strategy(mut self, strategy: RecoveryStrategy) -> Self {
        self.strategy = strategy;
        self
    }

    pub fn with_auto_checkpoint_interval(mut self, interval: usize) -> Self {
        self.auto_checkpoint_interval = interval;
        self
    }

    pub async fn init(&mut self, session_id: &str) -> AgentResult<Option<RecoveryState>> {
        self.checkpoint_manager.init().await?;

        if let Some(state) = self.load_recovery_state().await? {
            if state.session_id == session_id && state.crash_detected {
                return Ok(Some(state));
            }
        }

        let initial_state = RecoveryState {
            session_id: session_id.to_string(),
            last_checkpoint_id: None,
            pending_operations: Vec::new(),
            crash_detected: false,
            recovery_timestamp: current_timestamp(),
        };

        self.save_recovery_state(&initial_state).await?;
        Ok(None)
    }

    pub async fn start_operation(
        &mut self,
        op_type: &str,
        target: Option<PathBuf>,
    ) -> AgentResult<String> {
        let id = format!("op_{}_{}", current_timestamp(), self.operation_counter);
        self.operation_counter += 1;

        let op = PendingOperation {
            id: id.clone(),
            operation_type: op_type.to_string(),
            target_path: target,
            started_at: current_timestamp(),
            completed: false,
        };

        let mut state = self.load_or_create_state().await?;
        state.pending_operations.push(op);
        state.crash_detected = true;
        self.save_recovery_state(&state).await?;

        Ok(id)
    }

    pub async fn complete_operation(&mut self, op_id: &str) -> AgentResult<()> {
        let mut state = self.load_or_create_state().await?;

        if let Some(op) = state.pending_operations.iter_mut().find(|o| o.id == op_id) {
            op.completed = true;
        }

        state.pending_operations.retain(|o| !o.completed);

        if state.pending_operations.is_empty() {
            state.crash_detected = false;
        }

        self.save_recovery_state(&state).await?;

        self.operation_counter += 1;
        if self.auto_checkpoint_interval > 0
            && self.operation_counter % self.auto_checkpoint_interval == 0
        {
            self.create_auto_checkpoint().await?;
        }

        Ok(())
    }

    pub async fn create_checkpoint(&mut self, message: &str) -> AgentResult<CheckpointMeta> {
        let meta = self.checkpoint_manager.create(message).await?;

        let mut state = self.load_or_create_state().await?;
        state.last_checkpoint_id = Some(meta.id.clone());
        self.save_recovery_state(&state).await?;

        Ok(meta)
    }

    pub async fn create_labeled_checkpoint(
        &mut self,
        label: &str,
        message: &str,
    ) -> AgentResult<CheckpointMeta> {
        let meta = self
            .checkpoint_manager
            .create_labeled(label, message)
            .await?;

        let mut state = self.load_or_create_state().await?;
        state.last_checkpoint_id = Some(meta.id.clone());
        self.save_recovery_state(&state).await?;

        Ok(meta)
    }

    async fn create_auto_checkpoint(&mut self) -> AgentResult<CheckpointMeta> {
        self.create_checkpoint("Auto checkpoint").await
    }

    pub async fn recover(&mut self) -> AgentResult<RecoveryResult> {
        let state = match self.load_recovery_state().await? {
            Some(s) => s,
            None => {
                return Ok(RecoveryResult {
                    recovered: false,
                    checkpoint_restored: None,
                    operations_rolled_back: 0,
                    message: "No recovery state found".to_string(),
                })
            }
        };

        if !state.crash_detected {
            return Ok(RecoveryResult {
                recovered: false,
                checkpoint_restored: None,
                operations_rolled_back: 0,
                message: "No crash detected".to_string(),
            });
        }

        let checkpoint_id = match self.strategy {
            RecoveryStrategy::LastCheckpoint => state.last_checkpoint_id.clone(),
            RecoveryStrategy::LastLabeled => self
                .find_last_labeled_checkpoint()
                .await?
                .map(|c| c.id),
            RecoveryStrategy::Specific => state.last_checkpoint_id.clone(),
            RecoveryStrategy::Interactive => {
                return Ok(RecoveryResult {
                    recovered: false,
                    checkpoint_restored: None,
                    operations_rolled_back: state.pending_operations.len(),
                    message: "Interactive recovery required".to_string(),
                });
            }
        };

        let restored_id = if let Some(ref id) = checkpoint_id {
            self.checkpoint_manager.restore(id).await?;
            Some(id.clone())
        } else {
            None
        };

        let rolled_back = state.pending_operations.len();

        let new_state = RecoveryState {
            session_id: state.session_id,
            last_checkpoint_id: checkpoint_id.clone(),
            pending_operations: Vec::new(),
            crash_detected: false,
            recovery_timestamp: current_timestamp(),
        };
        self.save_recovery_state(&new_state).await?;

        Ok(RecoveryResult {
            recovered: true,
            checkpoint_restored: restored_id,
            operations_rolled_back: rolled_back,
            message: format!(
                "Recovered to checkpoint: {:?}",
                checkpoint_id.as_deref().unwrap_or("initial state")
            ),
        })
    }

    async fn find_last_labeled_checkpoint(&self) -> AgentResult<Option<CheckpointMeta>> {
        let checkpoints = self.checkpoint_manager.list().await;
        Ok(checkpoints.into_iter().rev().find(|c| c.label.is_some()))
    }

    pub async fn list_checkpoints(&self) -> Vec<CheckpointMeta> {
        self.checkpoint_manager.list().await
    }

    pub async fn get_recovery_state(&self) -> AgentResult<Option<RecoveryState>> {
        self.load_recovery_state().await
    }

    pub async fn clear_recovery_state(&mut self) -> AgentResult<()> {
        if self.state_file.exists() {
            fs::remove_file(&self.state_file)
                .await
                .map_err(|e| AgentError::io("remove recovery state", e))?;
        }
        Ok(())
    }

    async fn load_recovery_state(&self) -> AgentResult<Option<RecoveryState>> {
        if !self.state_file.exists() {
            return Ok(None);
        }

        let content = fs::read_to_string(&self.state_file)
            .await
            .map_err(|e| AgentError::io("read recovery state", e))?;

        let state: RecoveryState = serde_json::from_str(&content)
            .map_err(|e| AgentError::deserialization(e.to_string()))?;

        Ok(Some(state))
    }

    async fn load_or_create_state(&self) -> AgentResult<RecoveryState> {
        match self.load_recovery_state().await? {
            Some(state) => Ok(state),
            None => Ok(RecoveryState {
                session_id: String::new(),
                last_checkpoint_id: None,
                pending_operations: Vec::new(),
                crash_detected: false,
                recovery_timestamp: current_timestamp(),
            }),
        }
    }

    async fn save_recovery_state(&self, state: &RecoveryState) -> AgentResult<()> {
        let content = serde_json::to_string_pretty(state)
            .map_err(|e| AgentError::serialization(e.to_string()))?;

        fs::write(&self.state_file, content)
            .await
            .map_err(|e| AgentError::io("write recovery state", e))?;

        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct RecoveryResult {
    pub recovered: bool,
    pub checkpoint_restored: Option<String>,
    pub operations_rolled_back: usize,
    pub message: String,
}

impl RecoveryResult {
    pub fn success_message(&self) -> String {
        if self.recovered {
            format!(
                "Recovery successful: {} operations rolled back, restored to {:?}",
                self.operations_rolled_back,
                self.checkpoint_restored.as_deref().unwrap_or("initial")
            )
        } else {
            self.message.clone()
        }
    }
}

fn current_timestamp() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub struct IncrementalBackup {
    workspace: PathBuf,
    backup_dir: PathBuf,
    max_backups: usize,
}

impl IncrementalBackup {
    pub fn new(workspace: impl AsRef<Path>) -> Self {
        let workspace = workspace.as_ref().to_path_buf();
        let backup_dir = workspace.join(".agent-backups");

        Self {
            workspace,
            backup_dir,
            max_backups: 10,
        }
    }

    pub fn with_max_backups(mut self, max: usize) -> Self {
        self.max_backups = max;
        self
    }

    pub async fn init(&self) -> AgentResult<()> {
        if !self.backup_dir.exists() {
            fs::create_dir_all(&self.backup_dir)
                .await
                .map_err(|e| AgentError::io("create backup dir", e))?;
        }
        Ok(())
    }

    pub async fn backup_file(&self, rel_path: &str) -> AgentResult<()> {
        let source = self.workspace.join(rel_path);
        if !source.exists() {
            return Ok(());
        }

        let timestamp = current_timestamp();
        let backup_name = format!("{}_{}", rel_path.replace('/', "_"), timestamp);
        let backup_path = self.backup_dir.join(&backup_name);

        if let Some(parent) = backup_path.parent() {
            fs::create_dir_all(parent)
                .await
                .map_err(|e| AgentError::io("create backup parent dir", e))?;
        }

        fs::copy(&source, &backup_path)
            .await
            .map_err(|e| AgentError::io("copy file for backup", e))?;

        self.cleanup_old_backups(rel_path).await?;

        Ok(())
    }

    pub async fn restore_file(&self, rel_path: &str, backup_index: usize) -> AgentResult<()> {
        let backups = self.list_backups(rel_path).await?;

        if backup_index >= backups.len() {
            return Err(AgentError::not_found(format!(
                "backup index {} for {}",
                backup_index, rel_path
            )));
        }

        let backup_path = &backups[backup_index];
        let target = self.workspace.join(rel_path);

        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)
                .await
                .map_err(|e| AgentError::io("create target parent dir", e))?;
        }

        fs::copy(backup_path, &target)
            .await
            .map_err(|e| AgentError::io("restore from backup", e))?;

        Ok(())
    }

    pub async fn list_backups(&self, rel_path: &str) -> AgentResult<Vec<PathBuf>> {
        let prefix = rel_path.replace('/', "_");
        let mut backups = Vec::new();

        if !self.backup_dir.exists() {
            return Ok(backups);
        }

        let mut entries = fs::read_dir(&self.backup_dir)
            .await
            .map_err(|e| AgentError::io("read backup dir", e))?;

        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| AgentError::io("read backup entry", e))?
        {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with(&prefix) {
                backups.push(entry.path());
            }
        }

        backups.sort_by(|a, b| b.cmp(a));
        Ok(backups)
    }

    async fn cleanup_old_backups(&self, rel_path: &str) -> AgentResult<()> {
        let backups = self.list_backups(rel_path).await?;

        if backups.len() > self.max_backups {
            for backup in backups.iter().skip(self.max_backups) {
                let _ = fs::remove_file(backup).await;
            }
        }

        Ok(())
    }
}
