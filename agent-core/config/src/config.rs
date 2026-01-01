use agent_common::AgentResult;
use agent_protocol::ApprovalMode;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

use crate::features::Features;
use crate::provider::ProviderConfig;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub agent_home: PathBuf,
    pub working_directory: PathBuf,
    pub model_id: Option<String>,
    pub provider_id: String,
    pub providers: HashMap<String, ProviderConfig>,
    pub approval_mode: ApprovalMode,
    pub timeout_ms: u64,
    pub features: Features,
    pub tools: ToolsConfig,
}

impl Config {
    pub fn model_id(&self) -> AgentResult<&str> {
        if let Some(ref model_id) = self.model_id {
            return Ok(model_id);
        }

        let provider = self.provider()?;
        if provider.default_model.is_empty() {
            return Err(agent_common::AgentError::configuration(
                "no model configured",
            ));
        }

        Ok(&provider.default_model)
    }

    pub fn provider(&self) -> AgentResult<&ProviderConfig> {
        self.providers.get(&self.provider_id).ok_or_else(|| {
            agent_common::AgentError::not_found(format!("provider: {}", self.provider_id))
        })
    }

    pub fn is_tool_enabled(&self, tool_name: &str) -> bool {
        if self.tools.disabled.contains(&tool_name.to_string()) {
            return false;
        }
        true
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            agent_home: default_agent_home(),
            working_directory: std::env::current_dir().unwrap_or_default(),
            model_id: None,
            provider_id: String::from("anthropic"),
            providers: default_providers(),
            approval_mode: ApprovalMode::default(),
            timeout_ms: agent_common::DEFAULT_TIMEOUT_MS,
            features: Features::default(),
            tools: ToolsConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ToolsConfig {
    pub disabled: Vec<String>,
}

fn default_agent_home() -> PathBuf {
    dirs::home_dir()
        .map(|home| home.join(".cline"))
        .unwrap_or_else(|| PathBuf::from(".cline"))
}

fn default_providers() -> HashMap<String, ProviderConfig> {
    let mut providers = HashMap::new();

    providers.insert(
        String::from("anthropic"),
        ProviderConfig::new("Anthropic", "https://api.anthropic.com")
            .with_api_key_env("ANTHROPIC_API_KEY")
            .with_default_model("claude-sonnet-4-20250514"),
    );

    providers.insert(
        String::from("openai"),
        ProviderConfig::new("OpenAI", "https://api.openai.com")
            .with_api_key_env("OPENAI_API_KEY")
            .with_default_model("gpt-4o"),
    );

    providers
}

#[derive(Debug, Clone, Default)]
pub struct ConfigOverrides {
    pub model_id: Option<String>,
    pub provider_id: Option<String>,
    pub timeout_ms: Option<u64>,
    pub approval_mode: Option<ApprovalMode>,
    pub working_directory: Option<PathBuf>,
}

impl ConfigOverrides {
    pub fn apply(self, config: &mut Config) {
        if let Some(model_id) = self.model_id {
            config.model_id = Some(model_id);
        }
        if let Some(provider_id) = self.provider_id {
            config.provider_id = provider_id;
        }
        if let Some(timeout_ms) = self.timeout_ms {
            config.timeout_ms = timeout_ms;
        }
        if let Some(approval_mode) = self.approval_mode {
            config.approval_mode = approval_mode;
        }
        if let Some(working_directory) = self.working_directory {
            config.working_directory = working_directory;
        }
    }
}
