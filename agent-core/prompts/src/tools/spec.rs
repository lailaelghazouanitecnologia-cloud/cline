#![deny(clippy::all)]
#![forbid(unsafe_code)]

use crate::types::{ModelFamily, SystemPromptContext};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolId {
    ReadFile,
    WriteFile,
    ReplaceInFile,
    ApplyPatch,
    ExecuteCommand,
    SearchFiles,
    ListFiles,
    ListCodeDefinitions,
    BrowserAction,
    WebFetch,
    WebSearch,
    AskFollowup,
    AttemptCompletion,
    PlanRespond,
    ActRespond,
    McpTool,
    McpResource,
}

impl ToolId {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ReadFile => "read_file",
            Self::WriteFile => "write_to_file",
            Self::ReplaceInFile => "replace_in_file",
            Self::ApplyPatch => "apply_patch",
            Self::ExecuteCommand => "execute_command",
            Self::SearchFiles => "search_files",
            Self::ListFiles => "list_files",
            Self::ListCodeDefinitions => "list_code_definition_names",
            Self::BrowserAction => "browser_action",
            Self::WebFetch => "web_fetch",
            Self::WebSearch => "web_search",
            Self::AskFollowup => "ask_followup_question",
            Self::AttemptCompletion => "attempt_completion",
            Self::PlanRespond => "plan_mode_respond",
            Self::ActRespond => "act_mode_respond",
            Self::McpTool => "use_mcp_tool",
            Self::McpResource => "access_mcp_resource",
        }
    }
}

pub type ContextRequirementFn = fn(&SystemPromptContext) -> bool;
pub type InstructionFn = fn(&SystemPromptContext) -> String;

#[derive(Clone)]
pub struct ToolSpecParameter {
    pub name: String,
    pub required: bool,
    pub instruction: ParameterInstruction,
    pub usage: Option<String>,
    pub param_type: ParameterType,
    pub dependencies: Vec<ToolId>,
    pub context_requirements: Option<ContextRequirementFn>,
}

#[derive(Clone)]
pub enum ParameterInstruction {
    Static(String),
    Dynamic(InstructionFn),
}

impl ParameterInstruction {
    pub fn resolve(&self, context: &SystemPromptContext) -> String {
        match self {
            Self::Static(s) => s.clone(),
            Self::Dynamic(f) => f(context),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ParameterType {
    String,
    Boolean,
    Integer,
    Array,
    Object,
}

impl Default for ParameterType {
    fn default() -> Self {
        Self::String
    }
}

#[derive(Clone)]
pub struct ToolSpec {
    pub id: ToolId,
    pub variant: ModelFamily,
    pub name: String,
    pub description: String,
    pub parameters: Vec<ToolSpecParameter>,
    pub context_requirements: Option<ContextRequirementFn>,
}

impl ToolSpec {
    pub fn new(id: ToolId, variant: ModelFamily) -> Self {
        Self {
            id,
            variant,
            name: id.as_str().to_string(),
            description: String::new(),
            parameters: Vec::new(),
            context_requirements: None,
        }
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    pub fn with_parameter(mut self, param: ToolSpecParameter) -> Self {
        self.parameters.push(param);
        self
    }

    pub fn with_context_requirements(mut self, f: ContextRequirementFn) -> Self {
        self.context_requirements = Some(f);
        self
    }

    pub fn is_available(&self, context: &SystemPromptContext) -> bool {
        self.context_requirements
            .map(|f| f(context))
            .unwrap_or(true)
    }

    pub fn to_openai_tool(&self, context: &SystemPromptContext) -> Value {
        let mut properties: HashMap<String, Value> = HashMap::new();
        let mut required: Vec<String> = Vec::new();

        for param in self.get_available_parameters(context) {
            let instruction = param.instruction.resolve(context);
            let instruction = self.resolve_placeholders(&instruction, context);

            let mut prop = serde_json::json!({
                "type": format!("{:?}", param.param_type).to_lowercase(),
                "description": instruction
            });

            if let Some(ref usage) = param.usage {
                if let Some(obj) = prop.as_object_mut() {
                    obj.insert("example".to_string(), Value::String(usage.clone()));
                }
            }

            properties.insert(param.name.clone(), prop);

            if param.required {
                required.push(param.name.clone());
            }
        }

        serde_json::json!({
            "type": "function",
            "function": {
                "name": self.name,
                "description": self.resolve_placeholders(&self.description, context),
                "strict": false,
                "parameters": {
                    "type": "object",
                    "properties": properties,
                    "required": required,
                    "additionalProperties": false
                }
            }
        })
    }

    pub fn to_anthropic_tool(&self, context: &SystemPromptContext) -> Value {
        let mut properties: HashMap<String, Value> = HashMap::new();
        let mut required: Vec<String> = Vec::new();

        for param in self.get_available_parameters(context) {
            let instruction = param.instruction.resolve(context);
            let instruction = self.resolve_placeholders(&instruction, context);

            properties.insert(
                param.name.clone(),
                serde_json::json!({
                    "type": format!("{:?}", param.param_type).to_lowercase(),
                    "description": instruction
                }),
            );

            if param.required {
                required.push(param.name.clone());
            }
        }

        serde_json::json!({
            "name": self.name,
            "description": self.resolve_placeholders(&self.description, context),
            "input_schema": {
                "type": "object",
                "properties": properties,
                "required": required
            }
        })
    }

    pub fn to_xml_prompt(&self, context: &SystemPromptContext) -> String {
        let mut output = format!("## {}\n", self.name);
        output.push_str(&format!(
            "Description: {}\n",
            self.resolve_placeholders(&self.description, context)
        ));

        let params = self.get_available_parameters(context);

        if params.is_empty() {
            output.push_str("Parameters: None\n");
        } else {
            output.push_str("Parameters:\n");
            for param in &params {
                let req_text = if param.required { "required" } else { "optional" };
                let instruction = param.instruction.resolve(context);
                let instruction = self.resolve_placeholders(&instruction, context);
                output.push_str(&format!("- {}: ({}) {}\n", param.name, req_text, instruction));
            }
        }

        output.push_str("Usage:\n");
        output.push_str(&format!("<{}>\n", self.name));

        for param in &params {
            let usage = param.usage.as_deref().unwrap_or("...");
            output.push_str(&format!("<{}>{}</{}>\n", param.name, usage, param.name));
        }

        output.push_str(&format!("</{}>\n", self.name));

        output
    }

    fn get_available_parameters(&self, context: &SystemPromptContext) -> Vec<&ToolSpecParameter> {
        self.parameters
            .iter()
            .filter(|p| {
                p.context_requirements
                    .map(|f| f(context))
                    .unwrap_or(true)
            })
            .collect()
    }

    fn resolve_placeholders(&self, text: &str, context: &SystemPromptContext) -> String {
        text.replace("{{CWD}}", &context.cwd.display().to_string())
            .replace(
                "{{MULTI_ROOT_HINT}}",
                if context.multi_root_enabled {
                    " (when using multi-root, specify which workspace root)"
                } else {
                    ""
                },
            )
    }
}

impl ToolSpecParameter {
    pub fn new(name: impl Into<String>, instruction: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            required: true,
            instruction: ParameterInstruction::Static(instruction.into()),
            usage: None,
            param_type: ParameterType::String,
            dependencies: Vec::new(),
            context_requirements: None,
        }
    }

    pub fn optional(mut self) -> Self {
        self.required = false;
        self
    }

    pub fn with_usage(mut self, usage: impl Into<String>) -> Self {
        self.usage = Some(usage.into());
        self
    }

    pub fn with_type(mut self, param_type: ParameterType) -> Self {
        self.param_type = param_type;
        self
    }

    pub fn with_dynamic_instruction(mut self, f: InstructionFn) -> Self {
        self.instruction = ParameterInstruction::Dynamic(f);
        self
    }

    pub fn with_context_requirements(mut self, f: ContextRequirementFn) -> Self {
        self.context_requirements = Some(f);
        self
    }

    pub fn with_dependency(mut self, dep: ToolId) -> Self {
        self.dependencies.push(dep);
        self
    }
}

pub fn task_progress_parameter() -> ToolSpecParameter {
    ToolSpecParameter::new(
        "task_progress",
        "A checklist showing task progress. Mark completed tasks with [x] and remaining with [ ].",
    )
    .optional()
    .with_usage("- [x] Completed step\n- [ ] Next step")
}
