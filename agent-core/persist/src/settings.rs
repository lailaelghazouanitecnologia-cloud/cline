#![deny(clippy::all)]
#![forbid(unsafe_code)]

use agent_common::AgentResult;
use serde::{Deserialize, Serialize};

use crate::storage::StorageManager;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserSettings {
    pub model_id: String,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub approval_mode: ApprovalMode,
    pub max_tokens: u32,
    pub temperature: f32,
    pub auto_save_history: bool,
    pub custom_instructions: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalMode {
    Always,
    Dangerous,
    Never,
}

impl Default for UserSettings {
    fn default() -> Self {
        Self {
            model_id: "claude-3-5-sonnet-20241022".to_string(),
            api_key: None,
            base_url: None,
            approval_mode: ApprovalMode::Dangerous,
            max_tokens: 8192,
            temperature: 0.0,
            auto_save_history: true,
            custom_instructions: None,
        }
    }
}

impl UserSettings {
    pub fn with_model(mut self, model_id: impl Into<String>) -> Self {
        self.model_id = model_id.into();
        self
    }

    pub fn with_api_key(mut self, api_key: impl Into<String>) -> Self {
        self.api_key = Some(api_key.into());
        self
    }

    pub fn with_approval_mode(mut self, mode: ApprovalMode) -> Self {
        self.approval_mode = mode;
        self
    }

    pub fn with_custom_instructions(mut self, instructions: impl Into<String>) -> Self {
        self.custom_instructions = Some(instructions.into());
        self
    }
}

pub struct SettingsStore {
    storage: StorageManager,
}

impl SettingsStore {
    pub fn new(storage: StorageManager) -> Self {
        Self { storage }
    }

    pub async fn save(&self, settings: &UserSettings) -> AgentResult<()> {
        self.storage.write_json("settings", settings).await
    }

    pub async fn load(&self) -> AgentResult<UserSettings> {
        self.storage
            .read_json("settings")
            .await
            .map(|opt| opt.unwrap_or_default())
    }

    pub async fn reset(&self) -> AgentResult<()> {
        self.storage.delete("settings").await
    }
}

impl Default for SettingsStore {
    fn default() -> Self {
        Self::new(StorageManager::default())
    }
}
