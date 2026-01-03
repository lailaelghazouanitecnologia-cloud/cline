#![deny(clippy::all)]
#![forbid(unsafe_code)]

use crate::tools::spec::{ToolId, ToolSpec};
use crate::types::{ModelFamily, SystemPromptContext};
use std::collections::HashMap;

pub struct ToolRegistry {
    specs: HashMap<(ToolId, ModelFamily), ToolSpec>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            specs: HashMap::new(),
        };
        registry.load_all_specs();
        registry
    }

    fn load_all_specs(&mut self) {
        self.register_variants(super::read_file_variants());
        self.register_variants(super::write_file_variants());
        self.register_variants(super::replace_in_file_variants());
        self.register_variants(super::apply_patch_variants());
        self.register_variants(super::execute_command_variants());
        self.register_variants(super::search_files_variants());
        self.register_variants(super::list_files_variants());
        self.register_variants(super::list_code_definitions_variants());
        self.register_variants(super::browser_action_variants());
        self.register_variants(super::web_fetch_variants());
        self.register_variants(super::web_search_variants());
        self.register_variants(super::ask_followup_variants());
        self.register_variants(super::attempt_completion_variants());
        self.register_variants(super::plan_respond_variants());
        self.register_variants(super::act_respond_variants());
        self.register_variants(super::mcp_tool_variants());
    }

    fn register_variants(&mut self, variants: Vec<ToolSpec>) {
        for spec in variants {
            self.specs.insert((spec.id, spec.variant), spec);
        }
    }

    pub fn get(&self, id: ToolId, family: ModelFamily) -> Option<&ToolSpec> {
        self.specs
            .get(&(id, family))
            .or_else(|| self.specs.get(&(id, ModelFamily::Generic)))
    }

    pub fn get_for_context(
        &self,
        id: ToolId,
        context: &SystemPromptContext,
    ) -> Option<&ToolSpec> {
        let family = context.model_family();
        self.get(id, family)
    }

    pub fn get_all_for_family(&self, family: ModelFamily) -> Vec<&ToolSpec> {
        let mut tools: Vec<_> = self
            .specs
            .iter()
            .filter(|((_, f), _)| *f == family || *f == ModelFamily::Generic)
            .map(|(_, spec)| spec)
            .collect();

        tools.sort_by(|a, b| a.name.cmp(&b.name));
        tools.dedup_by(|a, b| a.id == b.id);
        tools
    }

    pub fn get_available_tools(&self, context: &SystemPromptContext) -> Vec<&ToolSpec> {
        let family = context.model_family();

        self.get_all_for_family(family)
            .into_iter()
            .filter(|spec| spec.is_available(context))
            .collect()
    }

    pub fn get_tool_prompts_xml(&self, context: &SystemPromptContext) -> String {
        self.get_available_tools(context)
            .iter()
            .map(|spec| spec.to_xml_prompt(context))
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn get_openai_tools(&self, context: &SystemPromptContext) -> Vec<serde_json::Value> {
        self.get_available_tools(context)
            .iter()
            .map(|spec| spec.to_openai_tool(context))
            .collect()
    }

    pub fn get_anthropic_tools(&self, context: &SystemPromptContext) -> Vec<serde_json::Value> {
        self.get_available_tools(context)
            .iter()
            .map(|spec| spec.to_anthropic_tool(context))
            .collect()
    }

    pub fn register(&mut self, spec: ToolSpec) {
        self.specs.insert((spec.id, spec.variant), spec);
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}
