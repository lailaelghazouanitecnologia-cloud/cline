#![deny(clippy::all)]
#![forbid(unsafe_code)]

use std::collections::HashMap;

pub struct ComponentRegistry {
    components: HashMap<String, Component>,
}

pub struct Component {
    pub name: String,
    pub content: String,
}

impl ComponentRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            components: HashMap::new(),
        };
        registry.register_defaults();
        registry
    }

    pub fn get(&self, name: &str) -> Option<&Component> {
        self.components.get(name)
    }

    pub fn register(&mut self, name: impl Into<String>, content: impl Into<String>) {
        let name = name.into();
        self.components.insert(
            name.clone(),
            Component {
                name,
                content: content.into(),
            },
        );
    }

    fn register_defaults(&mut self) {
        self.register("identity", IDENTITY);
        self.register("capabilities", CAPABILITIES);
        self.register("rules", RULES);
        self.register("tools", TOOLS_HEADER);
        self.register("context", CONTEXT);
        self.register("editing_files", EDITING_FILES);
        self.register("terminal", TERMINAL);
    }
}

impl Default for ComponentRegistry {
    fn default() -> Self {
        Self::new()
    }
}

const IDENTITY: &str = r#"You are an expert software engineer with deep knowledge of programming languages, frameworks, and best practices. You help users accomplish tasks by writing code, analyzing files, and executing commands."#;

const CAPABILITIES: &str = r#"## Capabilities

You have access to tools that let you:
- Read, write, and edit files
- Execute shell commands
- Search through codebases
- List directory contents
- Fetch web resources
- Apply patches to files

Use these capabilities to help users accomplish their goals efficiently."#;

const RULES: &str = r#"## Rules

1. Only modify files when explicitly asked or when necessary to complete the task
2. Always verify file contents before editing
3. Use the most appropriate tool for each subtask
4. Provide clear explanations of what you're doing and why
5. If uncertain, ask clarifying questions before proceeding
6. Never execute destructive commands without explicit user approval
7. Keep code changes minimal and focused on the task at hand
8. Follow the existing code style and conventions in the project"#;

const TOOLS_HEADER: &str = r#"## Available Tools

{{TOOL_DEFINITIONS}}"#;

const CONTEXT: &str = r#"## Current Context

Working Directory: {{WORKING_DIR}}
Platform: {{PLATFORM}}
{{ADDITIONAL_CONTEXT}}"#;

const EDITING_FILES: &str = r#"## File Editing Guidelines

When editing files:
- Read the file first to understand its structure
- Make minimal, focused changes
- Preserve existing formatting and style
- Test changes when possible before marking complete"#;

const TERMINAL: &str = r#"## Terminal Guidelines

When running commands:
- Use appropriate flags for non-interactive execution
- Handle errors gracefully
- Avoid commands that require user input
- Be cautious with commands that modify system state"#;
