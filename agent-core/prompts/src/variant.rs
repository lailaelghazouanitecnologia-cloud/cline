#![deny(clippy::all)]
#![forbid(unsafe_code)]

use crate::types::{ComponentOverride, ModelFamily, PromptConfig, PromptSection, SystemPromptContext};
use std::collections::HashMap;

pub type VariantMatcher = fn(&SystemPromptContext) -> bool;

#[derive(Clone)]
pub struct PromptVariant {
    pub id: String,
    pub version: u32,
    pub family: ModelFamily,
    pub description: String,
    pub tags: Vec<String>,
    pub labels: HashMap<String, u32>,

    pub config: PromptConfig,
    pub base_template: String,
    pub component_order: Vec<PromptSection>,
    pub component_overrides: HashMap<PromptSection, ComponentOverride>,
    pub placeholders: HashMap<String, String>,

    pub tools: Vec<String>,
    pub tool_overrides: HashMap<String, ComponentOverride>,

    matcher: Option<VariantMatcher>,
}

impl PromptVariant {
    pub fn new(family: ModelFamily) -> Self {
        Self {
            id: format!("{:?}", family).to_lowercase(),
            version: 1,
            family,
            description: String::new(),
            tags: Vec::new(),
            labels: HashMap::new(),
            config: PromptConfig::default(),
            base_template: DEFAULT_TEMPLATE.to_string(),
            component_order: PromptSection::default_order(),
            component_overrides: HashMap::new(),
            placeholders: HashMap::new(),
            tools: Vec::new(),
            tool_overrides: HashMap::new(),
            matcher: None,
        }
    }

    pub fn matches(&self, context: &SystemPromptContext) -> bool {
        if let Some(matcher) = self.matcher {
            return matcher(context);
        }

        let model_family = context.model_family();
        model_family == self.family
    }

    pub fn get_component_override(&self, section: PromptSection) -> Option<&ComponentOverride> {
        self.component_overrides.get(&section)
    }

    pub fn is_component_enabled(&self, section: PromptSection) -> bool {
        self.component_overrides
            .get(&section)
            .map(|o| o.enabled)
            .unwrap_or(true)
    }
}

const DEFAULT_TEMPLATE: &str = r#"{{AGENT_ROLE}}

====

{{SYSTEM_INFO}}

====

{{MCP}}

{{USER_INSTRUCTIONS}}

====

{{TOOL_USE}}

{{EDITING_FILES}}

====

{{CAPABILITIES}}

====

{{RULES}}

====

{{OBJECTIVE}}

{{ACT_VS_PLAN}}

{{TASK_PROGRESS}}"#;

pub struct VariantBuilder {
    variant: PromptVariant,
}

impl VariantBuilder {
    pub fn new(family: ModelFamily) -> Self {
        Self {
            variant: PromptVariant::new(family),
        }
    }

    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.variant.id = id.into();
        self
    }

    pub fn version(mut self, version: u32) -> Self {
        self.variant.version = version;
        self
    }

    pub fn description(mut self, desc: impl Into<String>) -> Self {
        self.variant.description = desc.into();
        self
    }

    pub fn tags(mut self, tags: Vec<String>) -> Self {
        self.variant.tags = tags;
        self
    }

    pub fn label(mut self, name: impl Into<String>, version: u32) -> Self {
        self.variant.labels.insert(name.into(), version);
        self
    }

    pub fn template(mut self, template: impl Into<String>) -> Self {
        self.variant.base_template = template.into();
        self
    }

    pub fn components(mut self, order: Vec<PromptSection>) -> Self {
        self.variant.component_order = order;
        self
    }

    pub fn override_component(mut self, section: PromptSection, override_config: ComponentOverride) -> Self {
        self.variant.component_overrides.insert(section, override_config);
        self
    }

    pub fn disable_component(mut self, section: PromptSection) -> Self {
        self.variant.component_overrides.insert(
            section,
            ComponentOverride {
                enabled: false,
                ..Default::default()
            },
        );
        self
    }

    pub fn placeholder(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.variant.placeholders.insert(key.into(), value.into());
        self
    }

    pub fn tools(mut self, tools: Vec<String>) -> Self {
        self.variant.tools = tools;
        self
    }

    pub fn matcher(mut self, matcher: VariantMatcher) -> Self {
        self.variant.matcher = Some(matcher);
        self
    }

    pub fn config(mut self, config: PromptConfig) -> Self {
        self.variant.config = config;
        self
    }

    pub fn build(self) -> PromptVariant {
        self.variant
    }
}

pub fn create_generic_variant() -> PromptVariant {
    VariantBuilder::new(ModelFamily::Generic)
        .id("generic")
        .description("Default variant for unknown models")
        .build()
}

pub fn create_claude_variant() -> PromptVariant {
    VariantBuilder::new(ModelFamily::Claude)
        .id("claude")
        .description("Optimized for Claude models")
        .matcher(|ctx| {
            let id = ctx.model_id.to_lowercase();
            id.contains("claude") && !id.contains("claude-4")
        })
        .build()
}

pub fn create_claude_next_gen_variant() -> PromptVariant {
    VariantBuilder::new(ModelFamily::ClaudeNextGen)
        .id("claude-next-gen")
        .description("Optimized for Claude 4 and newer models")
        .matcher(|ctx| {
            let id = ctx.model_id.to_lowercase();
            id.contains("claude-4") || id.contains("claude-opus-4")
        })
        .build()
}

pub fn create_gpt5_variant() -> PromptVariant {
    VariantBuilder::new(ModelFamily::Gpt5)
        .id("gpt-5")
        .description("Optimized for GPT-5 models")
        .matcher(|ctx| {
            let id = ctx.model_id.to_lowercase();
            id.contains("gpt-5") && !id.contains("gpt-5.1")
        })
        .build()
}

pub fn create_gpt51_variant() -> PromptVariant {
    VariantBuilder::new(ModelFamily::Gpt51)
        .id("gpt-5.1")
        .description("Optimized for GPT-5.1 models")
        .matcher(|ctx| {
            let id = ctx.model_id.to_lowercase();
            id.contains("gpt-5.1") || id.contains("gpt-51")
        })
        .build()
}

pub fn create_gemini_variant() -> PromptVariant {
    VariantBuilder::new(ModelFamily::Gemini)
        .id("gemini")
        .description("Optimized for Gemini models")
        .matcher(|ctx| {
            let id = ctx.model_id.to_lowercase();
            id.contains("gemini") && !id.contains("gemini-3")
        })
        .build()
}

pub fn create_gemini3_variant() -> PromptVariant {
    VariantBuilder::new(ModelFamily::Gemini3)
        .id("gemini-3")
        .description("Optimized for Gemini 3 models")
        .matcher(|ctx| {
            let id = ctx.model_id.to_lowercase();
            id.contains("gemini-3")
        })
        .build()
}

pub fn create_devstral_variant() -> PromptVariant {
    VariantBuilder::new(ModelFamily::Devstral)
        .id("devstral")
        .description("Optimized for Devstral models")
        .matcher(|ctx| ctx.model_id.to_lowercase().contains("devstral"))
        .build()
}

pub fn get_all_variants() -> Vec<PromptVariant> {
    vec![
        create_claude_next_gen_variant(),
        create_gpt51_variant(),
        create_gpt5_variant(),
        create_gemini3_variant(),
        create_claude_variant(),
        create_gemini_variant(),
        create_devstral_variant(),
        create_generic_variant(),
    ]
}
