#![deny(clippy::all)]
#![forbid(unsafe_code)]

use agent_common::AgentResult;
use serde::{Deserialize, Serialize};

use crate::storage::StorageManager;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct UserSettings {
    pub model: ModelSettings,
    pub provider: ProviderSettings,
    pub approval: ApprovalSettings,
    pub context: ContextSettings,
    pub shell: ShellSettings,
    pub browser: BrowserSettings,
    pub hooks: HookSettings,
    pub ui: UiSettings,
    pub custom_instructions: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ModelSettings {
    pub model_id: String,
    pub max_tokens: u32,
    pub temperature: f32,
    pub top_p: Option<f32>,
    pub thinking_enabled: bool,
    pub thinking_budget: Option<u32>,
    pub vision_enabled: bool,
}

impl Default for ModelSettings {
    fn default() -> Self {
        Self {
            model_id: "claude-3-5-sonnet-20241022".to_string(),
            max_tokens: 8192,
            temperature: 0.0,
            top_p: None,
            thinking_enabled: false,
            thinking_budget: None,
            vision_enabled: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ProviderSettings {
    pub provider_type: ProviderType,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub organization_id: Option<String>,
    pub project_id: Option<String>,
    pub region: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderType {
    Anthropic,
    OpenAi,
    Azure,
    Google,
    Bedrock,
    Vertex,
    Ollama,
    OpenRouter,
    Custom,
}

impl Default for ProviderSettings {
    fn default() -> Self {
        Self {
            provider_type: ProviderType::Anthropic,
            api_key: None,
            base_url: None,
            organization_id: None,
            project_id: None,
            region: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ApprovalSettings {
    pub mode: ApprovalMode,
    pub auto_approve_read: bool,
    pub auto_approve_write: bool,
    pub auto_approve_shell: bool,
    pub auto_approve_browser: bool,
    pub auto_approve_mcp: bool,
    pub dangerous_commands: Vec<String>,
    pub allowed_paths: Vec<String>,
    pub denied_paths: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalMode {
    Always,
    Dangerous,
    Never,
}

impl Default for ApprovalSettings {
    fn default() -> Self {
        Self {
            mode: ApprovalMode::Dangerous,
            auto_approve_read: true,
            auto_approve_write: false,
            auto_approve_shell: false,
            auto_approve_browser: false,
            auto_approve_mcp: false,
            dangerous_commands: vec![
                "rm".to_string(),
                "sudo".to_string(),
                "chmod".to_string(),
                "chown".to_string(),
                "kill".to_string(),
                "pkill".to_string(),
            ],
            allowed_paths: Vec::new(),
            denied_paths: vec![
                "/etc".to_string(),
                "/usr".to_string(),
                "/bin".to_string(),
                "/sbin".to_string(),
            ],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ContextSettings {
    pub max_context_tokens: usize,
    pub auto_compact: bool,
    pub compact_threshold: f32,
    pub preserve_recent_messages: usize,
    pub max_file_size_kb: usize,
    pub auto_save_history: bool,
    pub include_file_context: bool,
}

impl Default for ContextSettings {
    fn default() -> Self {
        Self {
            max_context_tokens: 128_000,
            auto_compact: true,
            compact_threshold: 0.80,
            preserve_recent_messages: 10,
            max_file_size_kb: 500,
            auto_save_history: true,
            include_file_context: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ShellSettings {
    pub default_shell: Option<String>,
    pub timeout_ms: u64,
    pub working_directory: Option<String>,
    pub inherit_env: bool,
    pub custom_env: Vec<EnvVar>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvVar {
    pub key: String,
    pub value: String,
}

impl Default for ShellSettings {
    fn default() -> Self {
        Self {
            default_shell: None,
            timeout_ms: 120_000,
            working_directory: None,
            inherit_env: true,
            custom_env: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BrowserSettings {
    pub enabled: bool,
    pub headless: bool,
    pub viewport_width: u32,
    pub viewport_height: u32,
    pub timeout_ms: u64,
    pub user_agent: Option<String>,
}

impl Default for BrowserSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            headless: true,
            viewport_width: 1280,
            viewport_height: 720,
            timeout_ms: 30_000,
            user_agent: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct HookSettings {
    pub enabled: bool,
    pub hooks_directory: Option<String>,
    pub default_timeout_ms: u64,
    pub fail_on_hook_error: bool,
}

impl Default for HookSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            hooks_directory: None,
            default_timeout_ms: 30_000,
            fail_on_hook_error: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct UiSettings {
    pub show_token_count: bool,
    pub show_cost_estimate: bool,
    pub syntax_highlighting: bool,
    pub diff_view_enabled: bool,
    pub confirm_before_apply: bool,
    pub theme: Option<String>,
}

impl Default for UiSettings {
    fn default() -> Self {
        Self {
            show_token_count: true,
            show_cost_estimate: true,
            syntax_highlighting: true,
            diff_view_enabled: true,
            confirm_before_apply: true,
            theme: None,
        }
    }
}

impl Default for UserSettings {
    fn default() -> Self {
        Self {
            model: ModelSettings::default(),
            provider: ProviderSettings::default(),
            approval: ApprovalSettings::default(),
            context: ContextSettings::default(),
            shell: ShellSettings::default(),
            browser: BrowserSettings::default(),
            hooks: HookSettings::default(),
            ui: UiSettings::default(),
            custom_instructions: None,
        }
    }
}

impl UserSettings {
    pub fn with_model_id(mut self, model_id: impl Into<String>) -> Self {
        self.model.model_id = model_id.into();
        self
    }

    pub fn with_api_key(mut self, api_key: impl Into<String>) -> Self {
        self.provider.api_key = Some(api_key.into());
        self
    }

    pub fn with_provider(mut self, provider_type: ProviderType) -> Self {
        self.provider.provider_type = provider_type;
        self
    }

    pub fn with_approval_mode(mut self, mode: ApprovalMode) -> Self {
        self.approval.mode = mode;
        self
    }

    pub fn with_custom_instructions(mut self, instructions: impl Into<String>) -> Self {
        self.custom_instructions = Some(instructions.into());
        self
    }

    pub fn with_max_tokens(mut self, tokens: u32) -> Self {
        self.model.max_tokens = tokens;
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

    pub async fn update<F>(&self, f: F) -> AgentResult<UserSettings>
    where
        F: FnOnce(&mut UserSettings),
    {
        let mut settings = self.load().await?;
        f(&mut settings);
        self.save(&settings).await?;
        Ok(settings)
    }
}

impl Default for SettingsStore {
    fn default() -> Self {
        Self::new(StorageManager::default())
    }
}
