#![deny(clippy::all)]
#![forbid(unsafe_code)]

use crate::component::ComponentRegistry;
use crate::template::Template;
use crate::variant::{ModelFamily, VariantConfig};
use std::collections::HashMap;
use std::path::Path;

pub struct PromptContext {
    working_dir: String,
    platform: String,
    tool_definitions: String,
    additional_context: Vec<String>,
    custom_instructions: Option<String>,
}

impl PromptContext {
    pub fn new(working_dir: impl AsRef<Path>) -> Self {
        Self {
            working_dir: working_dir.as_ref().display().to_string(),
            platform: std::env::consts::OS.to_string(),
            tool_definitions: String::new(),
            additional_context: Vec::new(),
            custom_instructions: None,
        }
    }

    pub fn with_tools(mut self, definitions: impl Into<String>) -> Self {
        self.tool_definitions = definitions.into();
        self
    }

    pub fn with_context(mut self, context: impl Into<String>) -> Self {
        self.additional_context.push(context.into());
        self
    }

    pub fn with_custom_instructions(mut self, instructions: impl Into<String>) -> Self {
        self.custom_instructions = Some(instructions.into());
        self
    }

    pub fn to_placeholders(&self) -> HashMap<String, String> {
        let mut map = HashMap::new();
        map.insert("WORKING_DIR".to_string(), self.working_dir.clone());
        map.insert("PLATFORM".to_string(), self.platform.clone());
        map.insert("TOOL_DEFINITIONS".to_string(), self.tool_definitions.clone());
        map.insert(
            "ADDITIONAL_CONTEXT".to_string(),
            self.additional_context.join("\n"),
        );
        if let Some(ref instructions) = self.custom_instructions {
            map.insert("CUSTOM_INSTRUCTIONS".to_string(), instructions.clone());
        }
        map
    }
}

pub struct PromptBuilder {
    registry: ComponentRegistry,
    config: VariantConfig,
}

impl PromptBuilder {
    pub fn new(model_id: &str) -> Self {
        let family = ModelFamily::from_model_id(model_id);
        Self {
            registry: ComponentRegistry::new(),
            config: VariantConfig::new(family),
        }
    }

    pub fn with_config(mut self, config: VariantConfig) -> Self {
        self.config = config;
        self
    }

    pub fn build(&self, context: &PromptContext) -> String {
        let placeholders = context.to_placeholders();
        let mut sections = Vec::new();

        for component_name in &self.config.enabled_components {
            let content = self.get_component_content(component_name);
            if let Some(content) = content {
                let template = Template::new(content);
                let rendered = self.apply_placeholders(&template.render(), &placeholders);
                sections.push(rendered);
            }
        }

        if let Some(ref custom) = context.custom_instructions {
            sections.push(format!("## Custom Instructions\n\n{}", custom));
        }

        sections.join("\n\n")
    }

    fn get_component_content(&self, name: &str) -> Option<String> {
        for override_item in &self.config.component_overrides {
            if override_item.component == name {
                return Some(override_item.content.clone());
            }
        }

        self.registry.get(name).map(|c| c.content.clone())
    }

    fn apply_placeholders(&self, content: &str, placeholders: &HashMap<String, String>) -> String {
        let mut result = content.to_string();
        for (key, value) in placeholders {
            result = result.replace(&format!("{{{{{}}}}}", key), value);
        }
        result
    }
}

impl Default for PromptBuilder {
    fn default() -> Self {
        Self::new("generic")
    }
}

pub fn build_system_prompt(model_id: &str, context: &PromptContext) -> String {
    PromptBuilder::new(model_id).build(context)
}

pub fn build_system_prompt_with_config(config: VariantConfig, context: &PromptContext) -> String {
    PromptBuilder::new("generic")
        .with_config(config)
        .build(context)
}
