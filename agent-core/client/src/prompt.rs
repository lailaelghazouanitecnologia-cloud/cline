#![deny(clippy::all)]
#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelVariant {
    Default,
    Planning,
    Coding,
    Reviewing,
    Debugging,
    Explaining,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentMode {
    Act,
    Plan,
    Architect,
    Ask,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelCapabilities {
    pub supports_images: bool,
    pub supports_computer_use: bool,
    pub max_tokens: usize,
    pub supports_streaming: bool,
    pub context_window: usize,
}

impl Default for ModelCapabilities {
    fn default() -> Self {
        Self {
            supports_images: true,
            supports_computer_use: true,
            max_tokens: 8192,
            supports_streaming: true,
            context_window: 200000,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemPromptConfig {
    pub persona: String,
    pub capabilities: Vec<String>,
    pub constraints: Vec<String>,
    pub tools_section: Option<String>,
    pub context_section: Option<String>,
    pub rules_section: Option<String>,
    pub variant: ModelVariant,
    pub mode: AgentMode,
}

impl Default for SystemPromptConfig {
    fn default() -> Self {
        Self {
            persona: "You are an AI coding assistant.".to_string(),
            capabilities: vec![
                "Read and analyze code files".to_string(),
                "Write and modify code".to_string(),
                "Execute shell commands".to_string(),
                "Search through codebases".to_string(),
            ],
            constraints: vec![
                "Always explain your reasoning".to_string(),
                "Ask for clarification when needed".to_string(),
                "Follow best practices and coding standards".to_string(),
            ],
            tools_section: None,
            context_section: None,
            rules_section: None,
            variant: ModelVariant::Default,
            mode: AgentMode::Act,
        }
    }
}

pub struct SystemPromptBuilder {
    config: SystemPromptConfig,
    sections: Vec<PromptSection>,
    variables: HashMap<String, String>,
}

#[derive(Debug, Clone)]
struct PromptSection {
    title: Option<String>,
    content: String,
    priority: i32,
}

impl SystemPromptBuilder {
    pub fn new() -> Self {
        Self {
            config: SystemPromptConfig::default(),
            sections: Vec::new(),
            variables: HashMap::new(),
        }
    }

    pub fn persona(mut self, persona: impl Into<String>) -> Self {
        self.config.persona = persona.into();
        self
    }

    pub fn add_capability(mut self, capability: impl Into<String>) -> Self {
        self.config.capabilities.push(capability.into());
        self
    }

    pub fn add_constraint(mut self, constraint: impl Into<String>) -> Self {
        self.config.constraints.push(constraint.into());
        self
    }

    pub fn section(mut self, title: impl Into<String>, content: impl Into<String>) -> Self {
        self.sections.push(PromptSection {
            title: Some(title.into()),
            content: content.into(),
            priority: 0,
        });
        self
    }

    pub fn section_with_priority(mut self, title: impl Into<String>, content: impl Into<String>, priority: i32) -> Self {
        self.sections.push(PromptSection {
            title: Some(title.into()),
            content: content.into(),
            priority,
        });
        self
    }

    pub fn raw_section(mut self, content: impl Into<String>) -> Self {
        self.sections.push(PromptSection {
            title: None,
            content: content.into(),
            priority: 0,
        });
        self
    }

    pub fn variable(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.variables.insert(key.into(), value.into());
        self
    }

    pub fn workspace_context(self, workspace: &Path) -> Self {
        let workspace_str = workspace.display().to_string();
        self.variable("WORKSPACE", &workspace_str)
            .section("Workspace", format!("Working directory: {}", workspace_str))
    }

    pub fn tool_descriptions(mut self, tools: &[(String, String)]) -> Self {
        if tools.is_empty() {
            return self;
        }

        let mut content = String::from("Available tools:\n\n");
        for (name, description) in tools {
            content.push_str(&format!("- **{}**: {}\n", name, description));
        }

        self.config.tools_section = Some(content);
        self
    }

    pub fn project_rules(mut self, rules: &str) -> Self {
        self.config.rules_section = Some(rules.to_string());
        self
    }

    pub fn variant(mut self, variant: ModelVariant) -> Self {
        self.config.variant = variant;
        self.apply_variant_defaults()
    }

    pub fn mode(mut self, mode: AgentMode) -> Self {
        self.config.mode = mode;
        self.apply_mode_defaults()
    }

    fn apply_variant_defaults(mut self) -> Self {
        match self.config.variant {
            ModelVariant::Planning => {
                self.config.persona = "You are a software architect and planner.".to_string();
                self.config.constraints.push(
                    "Focus on high-level design and break down complex tasks".to_string()
                );
            }
            ModelVariant::Coding => {
                self.config.persona = "You are an expert programmer.".to_string();
                self.config.constraints.push(
                    "Write clean, efficient, and well-tested code".to_string()
                );
            }
            ModelVariant::Reviewing => {
                self.config.persona = "You are a code reviewer.".to_string();
                self.config.constraints.push(
                    "Identify bugs, security issues, and improvement opportunities".to_string()
                );
            }
            ModelVariant::Debugging => {
                self.config.persona = "You are a debugging specialist.".to_string();
                self.config.constraints.push(
                    "Systematically identify root causes and provide fixes".to_string()
                );
            }
            ModelVariant::Explaining => {
                self.config.persona = "You are a technical educator.".to_string();
                self.config.constraints.push(
                    "Explain concepts clearly with examples".to_string()
                );
            }
            ModelVariant::Default => {}
        }
        self
    }

    fn apply_mode_defaults(mut self) -> Self {
        match self.config.mode {
            AgentMode::Act => {
                self.sections.push(PromptSection {
                    title: Some("Mode".to_string()),
                    content: "You are in ACT mode. Execute tasks directly using available tools.".to_string(),
                    priority: 100,
                });
            }
            AgentMode::Plan => {
                self.sections.push(PromptSection {
                    title: Some("Mode".to_string()),
                    content: "You are in PLAN mode. Create detailed implementation plans before acting.".to_string(),
                    priority: 100,
                });
            }
            AgentMode::Architect => {
                self.sections.push(PromptSection {
                    title: Some("Mode".to_string()),
                    content: "You are in ARCHITECT mode. Focus on system design and architecture.".to_string(),
                    priority: 100,
                });
            }
            AgentMode::Ask => {
                self.sections.push(PromptSection {
                    title: Some("Mode".to_string()),
                    content: "You are in ASK mode. Answer questions and provide information.".to_string(),
                    priority: 100,
                });
            }
        }
        self
    }

    pub fn build(mut self) -> String {
        let mut parts = Vec::new();

        parts.push(self.config.persona.clone());

        if !self.config.capabilities.is_empty() {
            let caps = self.config.capabilities
                .iter()
                .map(|c| format!("- {}", c))
                .collect::<Vec<_>>()
                .join("\n");
            parts.push(format!("\n## Capabilities\n\n{}", caps));
        }

        if !self.config.constraints.is_empty() {
            let cons = self.config.constraints
                .iter()
                .map(|c| format!("- {}", c))
                .collect::<Vec<_>>()
                .join("\n");
            parts.push(format!("\n## Guidelines\n\n{}", cons));
        }

        if let Some(tools) = &self.config.tools_section {
            parts.push(format!("\n## Tools\n\n{}", tools));
        }

        if let Some(context) = &self.config.context_section {
            parts.push(format!("\n## Context\n\n{}", context));
        }

        if let Some(rules) = &self.config.rules_section {
            parts.push(format!("\n## Project Rules\n\n{}", rules));
        }

        self.sections.sort_by(|a, b| b.priority.cmp(&a.priority));

        for section in &self.sections {
            let formatted = if let Some(title) = &section.title {
                format!("\n## {}\n\n{}", title, section.content)
            } else {
                format!("\n{}", section.content)
            };
            parts.push(formatted);
        }

        let mut result = parts.join("\n");

        for (key, value) in &self.variables {
            result = result.replace(&format!("{{{}}}", key), value);
        }

        result
    }
}

impl Default for SystemPromptBuilder {
    fn default() -> Self {
        Self::new()
    }
}

pub struct ContextInjector {
    max_context_size: usize,
    file_contents: Vec<FileContext>,
    code_snippets: Vec<CodeSnippet>,
    environment_info: Option<EnvironmentInfo>,
}

#[derive(Debug, Clone)]
pub struct FileContext {
    pub path: String,
    pub content: String,
    pub language: Option<String>,
    pub relevance: f32,
}

#[derive(Debug, Clone)]
pub struct CodeSnippet {
    pub source: String,
    pub content: String,
    pub start_line: usize,
    pub end_line: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentInfo {
    pub os: String,
    pub shell: String,
    pub working_dir: String,
    pub git_branch: Option<String>,
    pub git_status: Option<String>,
}

impl ContextInjector {
    pub fn new(max_size: usize) -> Self {
        Self {
            max_context_size: max_size,
            file_contents: Vec::new(),
            code_snippets: Vec::new(),
            environment_info: None,
        }
    }

    pub fn add_file(mut self, path: impl Into<String>, content: impl Into<String>) -> Self {
        self.file_contents.push(FileContext {
            path: path.into(),
            content: content.into(),
            language: None,
            relevance: 1.0,
        });
        self
    }

    pub fn add_file_with_language(
        mut self,
        path: impl Into<String>,
        content: impl Into<String>,
        language: impl Into<String>,
    ) -> Self {
        self.file_contents.push(FileContext {
            path: path.into(),
            content: content.into(),
            language: Some(language.into()),
            relevance: 1.0,
        });
        self
    }

    pub fn add_snippet(
        mut self,
        source: impl Into<String>,
        content: impl Into<String>,
        start_line: usize,
        end_line: usize,
    ) -> Self {
        self.code_snippets.push(CodeSnippet {
            source: source.into(),
            content: content.into(),
            start_line,
            end_line,
        });
        self
    }

    pub fn environment(mut self, info: EnvironmentInfo) -> Self {
        self.environment_info = Some(info);
        self
    }

    pub fn build(self) -> String {
        let mut parts = Vec::new();
        let mut current_size = 0;

        if let Some(env) = &self.environment_info {
            let env_section = format!(
                "Environment:\n- OS: {}\n- Shell: {}\n- Directory: {}{}{}",
                env.os,
                env.shell,
                env.working_dir,
                env.git_branch.as_ref().map(|b| format!("\n- Git branch: {}", b)).unwrap_or_default(),
                env.git_status.as_ref().map(|s| format!("\n- Git status: {}", s)).unwrap_or_default(),
            );
            current_size += env_section.len();
            parts.push(env_section);
        }

        let mut sorted_files = self.file_contents.clone();
        sorted_files.sort_by(|a, b| b.relevance.partial_cmp(&a.relevance).unwrap_or(std::cmp::Ordering::Equal));

        for file in sorted_files {
            let file_section = self.format_file(&file);
            if current_size + file_section.len() > self.max_context_size {
                break;
            }
            current_size += file_section.len();
            parts.push(file_section);
        }

        for snippet in &self.code_snippets {
            let snippet_section = self.format_snippet(snippet);
            if current_size + snippet_section.len() > self.max_context_size {
                break;
            }
            current_size += snippet_section.len();
            parts.push(snippet_section);
        }

        parts.join("\n\n")
    }

    fn format_file(&self, file: &FileContext) -> String {
        let lang = file.language.as_deref().unwrap_or("");
        format!(
            "File: {}\n```{}\n{}\n```",
            file.path, lang, file.content
        )
    }

    fn format_snippet(&self, snippet: &CodeSnippet) -> String {
        format!(
            "Snippet from {} (lines {}-{}):\n```\n{}\n```",
            snippet.source, snippet.start_line, snippet.end_line, snippet.content
        )
    }
}

impl Default for ContextInjector {
    fn default() -> Self {
        Self::new(100000)
    }
}

pub fn detect_language(path: &Path) -> Option<String> {
    let ext = path.extension()?.to_str()?;
    let lang = match ext {
        "rs" => "rust",
        "py" => "python",
        "js" => "javascript",
        "ts" => "typescript",
        "jsx" => "jsx",
        "tsx" => "tsx",
        "go" => "go",
        "java" => "java",
        "c" => "c",
        "cpp" | "cc" | "cxx" => "cpp",
        "h" | "hpp" => "cpp",
        "rb" => "ruby",
        "php" => "php",
        "swift" => "swift",
        "kt" | "kts" => "kotlin",
        "scala" => "scala",
        "sh" | "bash" => "bash",
        "zsh" => "zsh",
        "fish" => "fish",
        "ps1" => "powershell",
        "sql" => "sql",
        "html" => "html",
        "css" => "css",
        "scss" | "sass" => "scss",
        "less" => "less",
        "json" => "json",
        "yaml" | "yml" => "yaml",
        "toml" => "toml",
        "xml" => "xml",
        "md" => "markdown",
        "dockerfile" => "dockerfile",
        _ => return None,
    };
    Some(lang.to_string())
}

pub struct PromptPresets;

impl PromptPresets {
    pub fn coding_assistant() -> SystemPromptBuilder {
        SystemPromptBuilder::new()
            .variant(ModelVariant::Coding)
            .mode(AgentMode::Act)
            .add_constraint("Use idiomatic patterns for the target language")
            .add_constraint("Prioritize readability and maintainability")
    }

    pub fn code_reviewer() -> SystemPromptBuilder {
        SystemPromptBuilder::new()
            .variant(ModelVariant::Reviewing)
            .mode(AgentMode::Ask)
            .add_capability("Analyze code for bugs and security issues")
            .add_capability("Suggest performance improvements")
            .add_constraint("Be constructive and specific in feedback")
    }

    pub fn architect() -> SystemPromptBuilder {
        SystemPromptBuilder::new()
            .variant(ModelVariant::Planning)
            .mode(AgentMode::Architect)
            .add_capability("Design system architectures")
            .add_capability("Create implementation roadmaps")
            .add_constraint("Consider scalability and maintainability")
    }

    pub fn debugger() -> SystemPromptBuilder {
        SystemPromptBuilder::new()
            .variant(ModelVariant::Debugging)
            .mode(AgentMode::Act)
            .add_capability("Analyze error messages and stack traces")
            .add_capability("Identify root causes systematically")
            .add_constraint("Verify fixes before suggesting them")
    }

    pub fn teacher() -> SystemPromptBuilder {
        SystemPromptBuilder::new()
            .variant(ModelVariant::Explaining)
            .mode(AgentMode::Ask)
            .add_capability("Explain complex concepts simply")
            .add_capability("Provide practical examples")
            .add_constraint("Adjust explanations to user's level")
    }

    pub fn task_executor() -> SystemPromptBuilder {
        SystemPromptBuilder::new()
            .variant(ModelVariant::Default)
            .mode(AgentMode::Act)
            .add_constraint("Complete tasks efficiently")
            .add_constraint("Ask for clarification only when necessary")
    }

    pub fn planner() -> SystemPromptBuilder {
        SystemPromptBuilder::new()
            .variant(ModelVariant::Planning)
            .mode(AgentMode::Plan)
            .add_capability("Break down complex tasks into steps")
            .add_capability("Identify dependencies and risks")
            .add_constraint("Create actionable implementation plans")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptTemplate {
    pub name: String,
    pub description: String,
    pub template: String,
    pub variables: Vec<String>,
}

impl PromptTemplate {
    pub fn new(name: impl Into<String>, template: impl Into<String>) -> Self {
        let template_str = template.into();
        let variables = Self::extract_variables(&template_str);
        Self {
            name: name.into(),
            description: String::new(),
            template: template_str,
            variables,
        }
    }

    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }

    fn extract_variables(template: &str) -> Vec<String> {
        let mut vars = Vec::new();
        let mut chars = template.chars().peekable();

        while let Some(c) = chars.next() {
            if c == '{' {
                if let Some(&'{') = chars.peek() {
                    chars.next();
                    let mut var_name = String::new();
                    while let Some(&nc) = chars.peek() {
                        if nc == '}' {
                            chars.next();
                            if let Some(&'}') = chars.peek() {
                                chars.next();
                                if !var_name.is_empty() && !vars.contains(&var_name) {
                                    vars.push(var_name);
                                }
                            }
                            break;
                        }
                        var_name.push(nc);
                        chars.next();
                    }
                }
            }
        }

        vars
    }

    pub fn render(&self, values: &HashMap<String, String>) -> String {
        let mut result = self.template.clone();
        for (key, value) in values {
            result = result.replace(&format!("{{{{{}}}}}", key), value);
        }
        result
    }
}

pub struct TemplateRegistry {
    templates: HashMap<String, PromptTemplate>,
}

impl TemplateRegistry {
    pub fn new() -> Self {
        Self {
            templates: HashMap::new(),
        }
    }

    pub fn register(&mut self, template: PromptTemplate) {
        self.templates.insert(template.name.clone(), template);
    }

    pub fn get(&self, name: &str) -> Option<&PromptTemplate> {
        self.templates.get(name)
    }

    pub fn render(&self, name: &str, values: &HashMap<String, String>) -> Option<String> {
        self.templates.get(name).map(|t| t.render(values))
    }

    pub fn list(&self) -> Vec<&str> {
        self.templates.keys().map(|s| s.as_str()).collect()
    }
}

impl Default for TemplateRegistry {
    fn default() -> Self {
        let mut registry = Self::new();

        registry.register(
            PromptTemplate::new(
                "file_edit",
                "Edit the file at {{path}}:\n\n{{instructions}}"
            ).with_description("Template for file editing tasks")
        );

        registry.register(
            PromptTemplate::new(
                "code_review",
                "Review the following code for {{focus}}:\n\n```{{language}}\n{{code}}\n```"
            ).with_description("Template for code review requests")
        );

        registry.register(
            PromptTemplate::new(
                "bug_fix",
                "Fix the bug in {{file}}:\n\nError: {{error}}\n\nContext: {{context}}"
            ).with_description("Template for bug fixing tasks")
        );

        registry.register(
            PromptTemplate::new(
                "feature_request",
                "Implement the following feature:\n\n{{description}}\n\nRequirements:\n{{requirements}}"
            ).with_description("Template for feature implementation")
        );

        registry
    }
}
