#![deny(clippy::all)]
#![forbid(unsafe_code)]

use agent_common::{AgentError, AgentResult};
use agent_protocol::SessionState;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::fs;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSnapshot {
    pub session_id: String,
    pub state: SessionState,
    pub conversation: Vec<ConversationEntry>,
    pub working_directory: PathBuf,
    pub created_at: u64,
    pub updated_at: u64,
    pub metadata: SessionMetadata,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionMetadata {
    pub model_id: String,
    pub provider_id: String,
    pub task_description: Option<String>,
    pub total_tokens_used: usize,
    pub total_api_calls: usize,
    pub checkpoint_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationEntry {
    pub id: String,
    pub role: String,
    pub content: String,
    pub timestamp: u64,
    pub token_count: usize,
    pub tool_calls: Vec<ToolCallEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallEntry {
    pub id: String,
    pub tool_name: String,
    pub arguments: serde_json::Value,
    pub result: Option<String>,
    pub duration_ms: u64,
    pub success: bool,
}

pub struct SessionPersistence {
    storage_dir: PathBuf,
    auto_save: bool,
    save_interval_messages: usize,
}

impl SessionPersistence {
    pub fn new(storage_dir: impl AsRef<Path>) -> Self {
        Self {
            storage_dir: storage_dir.as_ref().to_path_buf(),
            auto_save: true,
            save_interval_messages: 5,
        }
    }

    pub fn with_auto_save(mut self, enabled: bool) -> Self {
        self.auto_save = enabled;
        self
    }

    pub fn with_interval(mut self, messages: usize) -> Self {
        self.save_interval_messages = messages;
        self
    }

    fn session_path(&self, session_id: &str) -> PathBuf {
        self.storage_dir.join(format!("{}.json", session_id))
    }

    fn sessions_index_path(&self) -> PathBuf {
        self.storage_dir.join("sessions.json")
    }

    pub async fn ensure_storage_dir(&self) -> AgentResult<()> {
        if !self.storage_dir.exists() {
            fs::create_dir_all(&self.storage_dir)
                .await
                .map_err(|e| AgentError::io("create storage dir", e))?;
        }
        Ok(())
    }

    pub async fn save(&self, snapshot: &SessionSnapshot) -> AgentResult<()> {
        self.ensure_storage_dir().await?;

        let path = self.session_path(&snapshot.session_id);
        let json = serde_json::to_string_pretty(snapshot)
            .map_err(|e| AgentError::serialization(e.to_string()))?;

        fs::write(&path, json)
            .await
            .map_err(|e| AgentError::io("save session", e))?;

        self.update_index(&snapshot.session_id, snapshot.updated_at)
            .await?;

        Ok(())
    }

    pub async fn load(&self, session_id: &str) -> AgentResult<SessionSnapshot> {
        let path = self.session_path(session_id);

        if !path.exists() {
            return Err(AgentError::not_found(format!("session: {}", session_id)));
        }

        let json = fs::read_to_string(&path)
            .await
            .map_err(|e| AgentError::io("read session", e))?;

        serde_json::from_str(&json)
            .map_err(|e| AgentError::deserialization(e.to_string()))
    }

    pub async fn delete(&self, session_id: &str) -> AgentResult<()> {
        let path = self.session_path(session_id);

        if path.exists() {
            fs::remove_file(&path)
                .await
                .map_err(|e| AgentError::io("delete session", e))?;
        }

        self.remove_from_index(session_id).await?;
        Ok(())
    }

    pub async fn list_sessions(&self) -> AgentResult<Vec<SessionSummary>> {
        let index_path = self.sessions_index_path();

        if !index_path.exists() {
            return Ok(Vec::new());
        }

        let json = fs::read_to_string(&index_path)
            .await
            .map_err(|e| AgentError::io("read sessions index", e))?;

        let index: SessionsIndex = serde_json::from_str(&json)
            .map_err(|e| AgentError::deserialization(e.to_string()))?;

        Ok(index.sessions)
    }

    pub async fn get_recent(&self, limit: usize) -> AgentResult<Vec<SessionSnapshot>> {
        let summaries = self.list_sessions().await?;
        let mut snapshots = Vec::new();

        for summary in summaries.into_iter().take(limit) {
            if let Ok(snapshot) = self.load(&summary.session_id).await {
                snapshots.push(snapshot);
            }
        }

        Ok(snapshots)
    }

    async fn update_index(&self, session_id: &str, updated_at: u64) -> AgentResult<()> {
        let index_path = self.sessions_index_path();
        let mut index = self.load_or_create_index().await?;

        if let Some(existing) = index
            .sessions
            .iter_mut()
            .find(|s| s.session_id == session_id)
        {
            existing.updated_at = updated_at;
        } else {
            index.sessions.push(SessionSummary {
                session_id: session_id.to_string(),
                updated_at,
            });
        }

        index.sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));

        let json = serde_json::to_string_pretty(&index)
            .map_err(|e| AgentError::serialization(e.to_string()))?;

        fs::write(&index_path, json)
            .await
            .map_err(|e| AgentError::io("write index", e))?;

        Ok(())
    }

    async fn remove_from_index(&self, session_id: &str) -> AgentResult<()> {
        let index_path = self.sessions_index_path();
        let mut index = self.load_or_create_index().await?;

        index.sessions.retain(|s| s.session_id != session_id);

        let json = serde_json::to_string_pretty(&index)
            .map_err(|e| AgentError::serialization(e.to_string()))?;

        fs::write(&index_path, json)
            .await
            .map_err(|e| AgentError::io("write index", e))?;

        Ok(())
    }

    async fn load_or_create_index(&self) -> AgentResult<SessionsIndex> {
        let index_path = self.sessions_index_path();

        if !index_path.exists() {
            return Ok(SessionsIndex::default());
        }

        let json = fs::read_to_string(&index_path)
            .await
            .map_err(|e| AgentError::io("read index", e))?;

        serde_json::from_str(&json).map_err(|e| AgentError::deserialization(e.to_string()))
    }

    pub fn should_auto_save(&self, message_count: usize) -> bool {
        self.auto_save && message_count > 0 && message_count % self.save_interval_messages == 0
    }
}

impl Default for SessionPersistence {
    fn default() -> Self {
        let storage_dir = dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("agent-core")
            .join("sessions");

        Self::new(storage_dir)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct SessionsIndex {
    sessions: Vec<SessionSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummary {
    pub session_id: String,
    pub updated_at: u64,
}

pub struct SessionBuilder {
    session_id: String,
    working_directory: PathBuf,
    model_id: String,
    provider_id: String,
}

impl SessionBuilder {
    pub fn new(session_id: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
            working_directory: PathBuf::from("."),
            model_id: String::new(),
            provider_id: String::new(),
        }
    }

    pub fn working_directory(mut self, dir: impl AsRef<Path>) -> Self {
        self.working_directory = dir.as_ref().to_path_buf();
        self
    }

    pub fn model(mut self, model_id: impl Into<String>) -> Self {
        self.model_id = model_id.into();
        self
    }

    pub fn provider(mut self, provider_id: impl Into<String>) -> Self {
        self.provider_id = provider_id.into();
        self
    }

    pub fn build(self) -> SessionSnapshot {
        let now = current_timestamp();
        SessionSnapshot {
            session_id: self.session_id,
            state: SessionState::Idle,
            conversation: Vec::new(),
            working_directory: self.working_directory,
            created_at: now,
            updated_at: now,
            metadata: SessionMetadata {
                model_id: self.model_id,
                provider_id: self.provider_id,
                ..Default::default()
            },
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

pub struct ProgressTracker {
    total_steps: usize,
    current_step: usize,
    step_descriptions: Vec<String>,
    completed_steps: Vec<bool>,
}

impl ProgressTracker {
    pub fn new() -> Self {
        Self {
            total_steps: 0,
            current_step: 0,
            step_descriptions: Vec::new(),
            completed_steps: Vec::new(),
        }
    }

    pub fn set_steps(&mut self, descriptions: Vec<String>) {
        self.total_steps = descriptions.len();
        self.step_descriptions = descriptions;
        self.completed_steps = vec![false; self.total_steps];
        self.current_step = 0;
    }

    pub fn start_step(&mut self, index: usize) {
        if index < self.total_steps {
            self.current_step = index;
        }
    }

    pub fn complete_step(&mut self, index: usize) {
        if index < self.completed_steps.len() {
            self.completed_steps[index] = true;
        }
    }

    pub fn current_description(&self) -> Option<&str> {
        self.step_descriptions.get(self.current_step).map(|s| s.as_str())
    }

    pub fn progress_percentage(&self) -> f32 {
        if self.total_steps == 0 {
            return 0.0;
        }
        let completed = self.completed_steps.iter().filter(|&&c| c).count();
        (completed as f32 / self.total_steps as f32) * 100.0
    }

    pub fn is_complete(&self) -> bool {
        self.completed_steps.iter().all(|&c| c)
    }

    pub fn summary(&self) -> String {
        let completed = self.completed_steps.iter().filter(|&&c| c).count();
        format!(
            "{}/{} steps complete ({:.0}%)",
            completed,
            self.total_steps,
            self.progress_percentage()
        )
    }
}

impl Default for ProgressTracker {
    fn default() -> Self {
        Self::new()
    }
}
