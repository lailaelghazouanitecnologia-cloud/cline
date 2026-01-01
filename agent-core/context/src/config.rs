#![deny(clippy::all)]
#![forbid(unsafe_code)]

use agent_common::{AgentError, AgentResult};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    pub model: ModelConfig,
    pub context: ContextConfig,
    pub tools: ToolsConfig,
    pub safety: SafetyConfig,
    pub paths: PathsConfig,
    pub behavior: BehaviorConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    pub default_model: String,
    pub temperature: f32,
    pub max_tokens: u32,
    pub thinking_budget: u32,
    pub streaming: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextConfig {
    pub max_context_tokens: u32,
    pub reserve_output_tokens: u32,
    pub auto_compact: bool,
    pub compact_threshold: f32,
    pub preserve_recent_messages: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolsConfig {
    pub enabled_tools: Vec<String>,
    pub disabled_tools: Vec<String>,
    pub timeout_secs: u64,
    pub max_parallel: usize,
    pub auto_approve: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafetyConfig {
    pub sandbox_enabled: bool,
    pub require_approval: bool,
    pub blocked_commands: Vec<String>,
    pub allowed_paths: Vec<PathBuf>,
    pub max_file_size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathsConfig {
    pub workspace: PathBuf,
    pub config_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub logs_dir: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BehaviorConfig {
    pub plan_mode_default: bool,
    pub auto_continue: bool,
    pub max_iterations: u32,
    pub verbose: bool,
    pub quiet: bool,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            model: ModelConfig::default(),
            context: ContextConfig::default(),
            tools: ToolsConfig::default(),
            safety: SafetyConfig::default(),
            paths: PathsConfig::default(),
            behavior: BehaviorConfig::default(),
        }
    }
}

impl Default for ModelConfig {
    fn default() -> Self {
        Self {
            default_model: "claude-sonnet-4-20250514".to_string(),
            temperature: 0.7,
            max_tokens: 8192,
            thinking_budget: 10000,
            streaming: true,
        }
    }
}

impl Default for ContextConfig {
    fn default() -> Self {
        Self {
            max_context_tokens: 128000,
            reserve_output_tokens: 8192,
            auto_compact: true,
            compact_threshold: 0.8,
            preserve_recent_messages: 5,
        }
    }
}

impl Default for ToolsConfig {
    fn default() -> Self {
        Self {
            enabled_tools: Vec::new(),
            disabled_tools: Vec::new(),
            timeout_secs: 300,
            max_parallel: 4,
            auto_approve: Vec::new(),
        }
    }
}

impl Default for SafetyConfig {
    fn default() -> Self {
        Self {
            sandbox_enabled: true,
            require_approval: true,
            blocked_commands: vec![
                "rm -rf /".to_string(),
                "mkfs".to_string(),
            ],
            allowed_paths: Vec::new(),
            max_file_size_bytes: 10 * 1024 * 1024,
        }
    }
}

impl Default for PathsConfig {
    fn default() -> Self {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

        Self {
            workspace: cwd,
            config_dir: home.join(".config/cline"),
            cache_dir: home.join(".cache/cline"),
            logs_dir: home.join(".local/share/cline/logs"),
        }
    }
}

impl Default for BehaviorConfig {
    fn default() -> Self {
        Self {
            plan_mode_default: false,
            auto_continue: true,
            max_iterations: 100,
            verbose: false,
            quiet: false,
        }
    }
}

#[derive(Debug, Clone)]
pub enum ConfigChange {
    ModelUpdated(ModelConfig),
    ContextUpdated(ContextConfig),
    ToolsUpdated(ToolsConfig),
    SafetyUpdated(SafetyConfig),
    FullReload(AgentConfig),
}

pub struct ConfigManager {
    config: Arc<RwLock<AgentConfig>>,
    sources: Vec<ConfigSource>,
    change_sender: Option<mpsc::Sender<ConfigChange>>,
    env_prefix: String,
}

#[derive(Debug, Clone)]
pub struct ConfigSource {
    pub path: PathBuf,
    pub priority: u8,
    pub format: ConfigFormat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigFormat {
    Json,
    Toml,
}

impl ConfigManager {
    pub fn new() -> Self {
        Self {
            config: Arc::new(RwLock::new(AgentConfig::default())),
            sources: Vec::new(),
            change_sender: None,
            env_prefix: "CLINE_".to_string(),
        }
    }

    pub fn with_env_prefix(mut self, prefix: &str) -> Self {
        self.env_prefix = prefix.to_string();
        self
    }

    pub fn add_source(&mut self, path: impl AsRef<Path>, priority: u8, format: ConfigFormat) {
        self.sources.push(ConfigSource {
            path: path.as_ref().to_path_buf(),
            priority,
            format,
        });
        self.sources.sort_by(|a, b| a.priority.cmp(&b.priority));
    }

    pub fn add_default_sources(&mut self, workspace: &Path) {
        if let Some(home) = dirs::home_dir() {
            self.add_source(home.join(".config/cline/config.json"), 1, ConfigFormat::Json);
        }

        self.add_source(workspace.join(".cline/config.json"), 2, ConfigFormat::Json);
        self.add_source(workspace.join("cline.json"), 3, ConfigFormat::Json);
    }

    pub fn subscribe(&mut self) -> mpsc::Receiver<ConfigChange> {
        let (tx, rx) = mpsc::channel(100);
        self.change_sender = Some(tx);
        rx
    }

    pub async fn load(&mut self) -> AgentResult<()> {
        let mut merged = AgentConfig::default();

        for source in &self.sources {
            if source.path.exists() {
                if let Ok(partial) = self.load_from_file(&source.path, source.format).await {
                    merged = self.merge_configs(merged, partial);
                }
            }
        }

        merged = self.apply_env_overrides(merged);

        let mut config = self.config.write().await;
        *config = merged.clone();

        if let Some(ref tx) = self.change_sender {
            let _ = tx.send(ConfigChange::FullReload(merged)).await;
        }

        Ok(())
    }

    async fn load_from_file(&self, path: &Path, format: ConfigFormat) -> AgentResult<PartialConfig> {
        let content = tokio::fs::read_to_string(path)
            .await
            .map_err(|e| AgentError::io("read config", e))?;

        match format {
            ConfigFormat::Json => serde_json::from_str(&content)
                .map_err(|e| AgentError::deserialization(e.to_string())),
            ConfigFormat::Toml => toml::from_str(&content)
                .map_err(|e| AgentError::deserialization(e.to_string())),
        }
    }

    fn merge_configs(&self, base: AgentConfig, partial: PartialConfig) -> AgentConfig {
        let mut result = base;

        if let Some(model) = partial.model {
            if let Some(v) = model.default_model {
                result.model.default_model = v;
            }
            if let Some(v) = model.temperature {
                result.model.temperature = v;
            }
            if let Some(v) = model.max_tokens {
                result.model.max_tokens = v;
            }
            if let Some(v) = model.thinking_budget {
                result.model.thinking_budget = v;
            }
            if let Some(v) = model.streaming {
                result.model.streaming = v;
            }
        }

        if let Some(ctx) = partial.context {
            if let Some(v) = ctx.max_context_tokens {
                result.context.max_context_tokens = v;
            }
            if let Some(v) = ctx.auto_compact {
                result.context.auto_compact = v;
            }
        }

        if let Some(tools) = partial.tools {
            if let Some(v) = tools.enabled_tools {
                result.tools.enabled_tools = v;
            }
            if let Some(v) = tools.disabled_tools {
                result.tools.disabled_tools = v;
            }
            if let Some(v) = tools.timeout_secs {
                result.tools.timeout_secs = v;
            }
            if let Some(v) = tools.auto_approve {
                result.tools.auto_approve = v;
            }
        }

        if let Some(safety) = partial.safety {
            if let Some(v) = safety.sandbox_enabled {
                result.safety.sandbox_enabled = v;
            }
            if let Some(v) = safety.require_approval {
                result.safety.require_approval = v;
            }
            if let Some(v) = safety.blocked_commands {
                result.safety.blocked_commands = v;
            }
        }

        if let Some(behavior) = partial.behavior {
            if let Some(v) = behavior.plan_mode_default {
                result.behavior.plan_mode_default = v;
            }
            if let Some(v) = behavior.auto_continue {
                result.behavior.auto_continue = v;
            }
            if let Some(v) = behavior.max_iterations {
                result.behavior.max_iterations = v;
            }
        }

        result
    }

    fn apply_env_overrides(&self, mut config: AgentConfig) -> AgentConfig {
        if let Ok(v) = std::env::var(format!("{}MODEL", self.env_prefix)) {
            config.model.default_model = v;
        }

        if let Ok(v) = std::env::var(format!("{}MAX_TOKENS", self.env_prefix)) {
            if let Ok(n) = v.parse() {
                config.model.max_tokens = n;
            }
        }

        if let Ok(v) = std::env::var(format!("{}SANDBOX", self.env_prefix)) {
            config.safety.sandbox_enabled = v.to_lowercase() == "true" || v == "1";
        }

        if let Ok(v) = std::env::var(format!("{}VERBOSE", self.env_prefix)) {
            config.behavior.verbose = v.to_lowercase() == "true" || v == "1";
        }

        if let Ok(v) = std::env::var(format!("{}WORKSPACE", self.env_prefix)) {
            config.paths.workspace = PathBuf::from(v);
        }

        config
    }

    pub async fn get(&self) -> AgentConfig {
        self.config.read().await.clone()
    }

    pub async fn get_model(&self) -> ModelConfig {
        self.config.read().await.model.clone()
    }

    pub async fn get_context(&self) -> ContextConfig {
        self.config.read().await.context.clone()
    }

    pub async fn get_tools(&self) -> ToolsConfig {
        self.config.read().await.tools.clone()
    }

    pub async fn get_safety(&self) -> SafetyConfig {
        self.config.read().await.safety.clone()
    }

    pub async fn get_paths(&self) -> PathsConfig {
        self.config.read().await.paths.clone()
    }

    pub async fn get_behavior(&self) -> BehaviorConfig {
        self.config.read().await.behavior.clone()
    }

    pub async fn update_model(&self, model: ModelConfig) {
        let mut config = self.config.write().await;
        config.model = model.clone();

        if let Some(ref tx) = self.change_sender {
            let _ = tx.send(ConfigChange::ModelUpdated(model)).await;
        }
    }

    pub async fn update_safety(&self, safety: SafetyConfig) {
        let mut config = self.config.write().await;
        config.safety = safety.clone();

        if let Some(ref tx) = self.change_sender {
            let _ = tx.send(ConfigChange::SafetyUpdated(safety)).await;
        }
    }

    pub async fn is_tool_enabled(&self, tool_name: &str) -> bool {
        let config = self.config.read().await;

        if config.tools.disabled_tools.contains(&tool_name.to_string()) {
            return false;
        }

        if config.tools.enabled_tools.is_empty() {
            return true;
        }

        config.tools.enabled_tools.contains(&tool_name.to_string())
    }

    pub async fn is_auto_approve(&self, tool_name: &str) -> bool {
        let config = self.config.read().await;
        config.tools.auto_approve.contains(&tool_name.to_string())
    }

    pub async fn is_path_allowed(&self, path: &Path) -> bool {
        let config = self.config.read().await;

        if config.safety.allowed_paths.is_empty() {
            return true;
        }

        for allowed in &config.safety.allowed_paths {
            if path.starts_with(allowed) {
                return true;
            }
        }

        false
    }
}

impl Default for ConfigManager {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct PartialConfig {
    model: Option<PartialModelConfig>,
    context: Option<PartialContextConfig>,
    tools: Option<PartialToolsConfig>,
    safety: Option<PartialSafetyConfig>,
    behavior: Option<PartialBehaviorConfig>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct PartialModelConfig {
    default_model: Option<String>,
    temperature: Option<f32>,
    max_tokens: Option<u32>,
    thinking_budget: Option<u32>,
    streaming: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct PartialContextConfig {
    max_context_tokens: Option<u32>,
    reserve_output_tokens: Option<u32>,
    auto_compact: Option<bool>,
    compact_threshold: Option<f32>,
    preserve_recent_messages: Option<u32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct PartialToolsConfig {
    enabled_tools: Option<Vec<String>>,
    disabled_tools: Option<Vec<String>>,
    timeout_secs: Option<u64>,
    max_parallel: Option<usize>,
    auto_approve: Option<Vec<String>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct PartialSafetyConfig {
    sandbox_enabled: Option<bool>,
    require_approval: Option<bool>,
    blocked_commands: Option<Vec<String>>,
    allowed_paths: Option<Vec<PathBuf>>,
    max_file_size_bytes: Option<u64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct PartialBehaviorConfig {
    plan_mode_default: Option<bool>,
    auto_continue: Option<bool>,
    max_iterations: Option<u32>,
    verbose: Option<bool>,
    quiet: Option<bool>,
}
