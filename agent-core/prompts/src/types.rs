#![deny(clippy::all)]
#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelFamily {
    Generic,
    Claude,
    ClaudeNextGen,
    Gpt4,
    Gpt5,
    Gpt51,
    Gemini,
    Gemini3,
    Devstral,
    DeepSeek,
    Qwen,
    Hermes,
    Local,
}

impl ModelFamily {
    pub fn from_model_id(model_id: &str) -> Self {
        let lower = model_id.to_lowercase();

        if lower.contains("claude-4") || lower.contains("claude-opus-4") {
            Self::ClaudeNextGen
        } else if lower.contains("claude") {
            Self::Claude
        } else if lower.contains("gpt-5.1") || lower.contains("gpt-51") {
            Self::Gpt51
        } else if lower.contains("gpt-5") {
            Self::Gpt5
        } else if lower.contains("gpt-4") || lower.contains("gpt4") {
            Self::Gpt4
        } else if lower.contains("gemini-3") {
            Self::Gemini3
        } else if lower.contains("gemini") {
            Self::Gemini
        } else if lower.contains("devstral") {
            Self::Devstral
        } else if lower.contains("deepseek") {
            Self::DeepSeek
        } else if lower.contains("qwen") {
            Self::Qwen
        } else if lower.contains("hermes") {
            Self::Hermes
        } else if lower.contains("llama") || lower.contains("mistral") {
            Self::Local
        } else {
            Self::Generic
        }
    }

    pub fn supports_extended_thinking(&self) -> bool {
        matches!(self, Self::Claude | Self::ClaudeNextGen | Self::Gpt5 | Self::Gpt51)
    }

    pub fn supports_vision(&self) -> bool {
        matches!(
            self,
            Self::Claude
                | Self::ClaudeNextGen
                | Self::Gpt4
                | Self::Gpt5
                | Self::Gpt51
                | Self::Gemini
                | Self::Gemini3
        )
    }

    pub fn supports_native_tools(&self) -> bool {
        matches!(
            self,
            Self::Claude
                | Self::ClaudeNextGen
                | Self::Gpt4
                | Self::Gpt5
                | Self::Gpt51
                | Self::Gemini
                | Self::Gemini3
        )
    }

    pub fn supports_parallel_tools(&self) -> bool {
        matches!(self, Self::ClaudeNextGen | Self::Gpt5 | Self::Gpt51)
    }

    pub fn preferred_tool_format(&self) -> ToolFormat {
        match self {
            Self::Claude | Self::ClaudeNextGen => ToolFormat::Xml,
            Self::Gpt4 | Self::Gpt5 | Self::Gpt51 => ToolFormat::Json,
            Self::Gemini | Self::Gemini3 => ToolFormat::Json,
            Self::Devstral | Self::Hermes => ToolFormat::Xml,
            _ => ToolFormat::Xml,
        }
    }

    pub fn default_context_window(&self) -> usize {
        match self {
            Self::Claude | Self::ClaudeNextGen => 200_000,
            Self::Gpt5 | Self::Gpt51 => 128_000,
            Self::Gpt4 => 128_000,
            Self::Gemini | Self::Gemini3 => 128_000,
            Self::DeepSeek => 64_000,
            Self::Qwen => 32_000,
            _ => 16_000,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolFormat {
    Json,
    Xml,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PromptSection {
    AgentRole,
    SystemInfo,
    Capabilities,
    Rules,
    ToolUse,
    EditingFiles,
    Objective,
    ActVsPlan,
    Mcp,
    UserInstructions,
    Feedback,
    TaskProgress,
    CliSubagents,
}

impl PromptSection {
    pub fn all() -> &'static [Self] {
        &[
            Self::AgentRole,
            Self::SystemInfo,
            Self::Capabilities,
            Self::Rules,
            Self::ToolUse,
            Self::EditingFiles,
            Self::Objective,
            Self::ActVsPlan,
            Self::Mcp,
            Self::UserInstructions,
            Self::TaskProgress,
        ]
    }

    pub fn default_order() -> Vec<Self> {
        vec![
            Self::AgentRole,
            Self::SystemInfo,
            Self::Mcp,
            Self::UserInstructions,
            Self::ToolUse,
            Self::EditingFiles,
            Self::Capabilities,
            Self::Rules,
            Self::Objective,
            Self::ActVsPlan,
            Self::TaskProgress,
        ]
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::AgentRole => "agent_role",
            Self::SystemInfo => "system_info",
            Self::Capabilities => "capabilities",
            Self::Rules => "rules",
            Self::ToolUse => "tool_use",
            Self::EditingFiles => "editing_files",
            Self::Objective => "objective",
            Self::ActVsPlan => "act_vs_plan",
            Self::Mcp => "mcp",
            Self::UserInstructions => "user_instructions",
            Self::Feedback => "feedback",
            Self::TaskProgress => "task_progress",
            Self::CliSubagents => "cli_subagents",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserSettings {
    pub enabled: bool,
    pub viewport_width: u32,
    pub viewport_height: u32,
    pub headless: bool,
}

impl Default for BrowserSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            viewport_width: 1280,
            viewport_height: 800,
            headless: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceRoot {
    pub path: PathBuf,
    pub name: String,
    pub vcs: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SystemPromptContext {
    pub model_id: String,
    pub model_family: Option<ModelFamily>,
    pub cwd: PathBuf,
    pub platform: String,
    pub ide: String,

    pub supports_browser: bool,
    pub browser_settings: Option<BrowserSettings>,

    pub mcp_servers: Vec<String>,
    pub mcp_tools: Vec<String>,

    pub global_rules: Option<String>,
    pub local_rules: Option<String>,
    pub cursor_rules: Option<String>,
    pub windsurf_rules: Option<String>,
    pub cline_ignore: Option<String>,

    pub preferred_language: Option<String>,
    pub custom_instructions: Option<String>,

    pub yolo_mode: bool,
    pub web_tools_enabled: bool,
    pub multi_root_enabled: bool,
    pub workspace_roots: Vec<WorkspaceRoot>,

    pub subagents_enabled: bool,
    pub is_cli_subagent: bool,
    pub enable_native_tools: bool,
    pub enable_parallel_tools: bool,

    pub terminal_mode: TerminalMode,

    pub runtime_placeholders: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminalMode {
    #[default]
    VscodeTerminal,
    BackgroundExec,
}

impl SystemPromptContext {
    pub fn new(cwd: impl Into<PathBuf>) -> Self {
        Self {
            cwd: cwd.into(),
            platform: std::env::consts::OS.to_string(),
            ide: "vscode".to_string(),
            ..Default::default()
        }
    }

    pub fn with_model(mut self, model_id: impl Into<String>) -> Self {
        let id = model_id.into();
        self.model_family = Some(ModelFamily::from_model_id(&id));
        self.model_id = id;
        self
    }

    pub fn with_browser(mut self, enabled: bool) -> Self {
        self.supports_browser = enabled;
        self
    }

    pub fn with_yolo_mode(mut self, enabled: bool) -> Self {
        self.yolo_mode = enabled;
        self
    }

    pub fn with_mcp_servers(mut self, servers: Vec<String>) -> Self {
        self.mcp_servers = servers;
        self
    }

    pub fn with_global_rules(mut self, rules: impl Into<String>) -> Self {
        self.global_rules = Some(rules.into());
        self
    }

    pub fn with_local_rules(mut self, rules: impl Into<String>) -> Self {
        self.local_rules = Some(rules.into());
        self
    }

    pub fn with_custom_instructions(mut self, instructions: impl Into<String>) -> Self {
        self.custom_instructions = Some(instructions.into());
        self
    }

    pub fn model_family(&self) -> ModelFamily {
        self.model_family.unwrap_or_else(|| ModelFamily::from_model_id(&self.model_id))
    }

    pub fn get_placeholder(&self, key: &str) -> Option<&serde_json::Value> {
        self.runtime_placeholders.get(key)
    }

    pub fn set_placeholder(&mut self, key: impl Into<String>, value: serde_json::Value) {
        self.runtime_placeholders.insert(key.into(), value);
    }
}

#[derive(Debug, Clone)]
pub struct ComponentOverride {
    pub template: Option<String>,
    pub enabled: bool,
    pub order: Option<i32>,
}

impl Default for ComponentOverride {
    fn default() -> Self {
        Self {
            template: None,
            enabled: true,
            order: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PromptConfig {
    pub model_name: Option<String>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<usize>,
}

impl Default for PromptConfig {
    fn default() -> Self {
        Self {
            model_name: None,
            temperature: Some(0.0),
            max_tokens: None,
        }
    }
}
