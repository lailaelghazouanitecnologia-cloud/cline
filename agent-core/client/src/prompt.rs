#![deny(clippy::all)]
#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemPromptConfig {
    pub persona: String,
    pub capabilities: Vec<String>,
    pub constraints: Vec<String>,
    pub tools_section: Option<String>,
    pub context_section: Option<String>,
    pub rules_section: Option<String>,
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
