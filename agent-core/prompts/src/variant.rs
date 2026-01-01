#![deny(clippy::all)]
#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelFamily {
    Generic,
    Claude,
    Gpt4,
    Gpt5,
    Gemini,
    Local,
}

impl ModelFamily {
    pub fn from_model_id(model_id: &str) -> Self {
        let lower = model_id.to_lowercase();

        if lower.contains("claude") {
            Self::Claude
        } else if lower.contains("gpt-5") {
            Self::Gpt5
        } else if lower.contains("gpt-4") || lower.contains("gpt4") {
            Self::Gpt4
        } else if lower.contains("gemini") {
            Self::Gemini
        } else if lower.contains("llama")
            || lower.contains("mistral")
            || lower.contains("qwen")
            || lower.contains("deepseek")
        {
            Self::Local
        } else {
            Self::Generic
        }
    }

    pub fn supports_extended_thinking(&self) -> bool {
        matches!(self, Self::Claude | Self::Gpt5)
    }

    pub fn supports_vision(&self) -> bool {
        matches!(self, Self::Claude | Self::Gpt4 | Self::Gpt5 | Self::Gemini)
    }

    pub fn supports_function_calling(&self) -> bool {
        matches!(self, Self::Claude | Self::Gpt4 | Self::Gpt5 | Self::Gemini)
    }

    pub fn preferred_tool_format(&self) -> ToolFormat {
        match self {
            Self::Claude => ToolFormat::Xml,
            Self::Gpt4 | Self::Gpt5 => ToolFormat::Json,
            Self::Gemini => ToolFormat::Json,
            Self::Local => ToolFormat::Xml,
            Self::Generic => ToolFormat::Json,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolFormat {
    Json,
    Xml,
}

#[derive(Debug, Clone)]
pub struct VariantConfig {
    pub family: ModelFamily,
    pub max_tokens: u32,
    pub temperature: f32,
    pub enabled_components: Vec<String>,
    pub component_overrides: Vec<ComponentOverride>,
}

#[derive(Debug, Clone)]
pub struct ComponentOverride {
    pub component: String,
    pub content: String,
}

impl VariantConfig {
    pub fn new(family: ModelFamily) -> Self {
        Self {
            family,
            max_tokens: Self::default_max_tokens(family),
            temperature: 0.0,
            enabled_components: Self::default_components(),
            component_overrides: Vec::new(),
        }
    }

    pub fn with_max_tokens(mut self, tokens: u32) -> Self {
        self.max_tokens = tokens;
        self
    }

    pub fn with_temperature(mut self, temp: f32) -> Self {
        self.temperature = temp;
        self
    }

    pub fn with_component(mut self, component: impl Into<String>) -> Self {
        self.enabled_components.push(component.into());
        self
    }

    pub fn with_override(mut self, component: impl Into<String>, content: impl Into<String>) -> Self {
        self.component_overrides.push(ComponentOverride {
            component: component.into(),
            content: content.into(),
        });
        self
    }

    fn default_max_tokens(family: ModelFamily) -> u32 {
        match family {
            ModelFamily::Claude => 200_000,
            ModelFamily::Gpt5 => 128_000,
            ModelFamily::Gpt4 => 128_000,
            ModelFamily::Gemini => 128_000,
            ModelFamily::Local => 32_000,
            ModelFamily::Generic => 16_000,
        }
    }

    fn default_components() -> Vec<String> {
        vec![
            "identity".to_string(),
            "capabilities".to_string(),
            "rules".to_string(),
            "tools".to_string(),
            "context".to_string(),
        ]
    }
}

impl Default for VariantConfig {
    fn default() -> Self {
        Self::new(ModelFamily::Generic)
    }
}
