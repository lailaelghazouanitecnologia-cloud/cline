#![deny(clippy::all)]
#![forbid(unsafe_code)]

use crate::types::SystemPromptContext;
use std::collections::HashMap;

pub struct TemplateEngine;

impl TemplateEngine {
    pub fn resolve(
        template: &str,
        context: &SystemPromptContext,
        placeholders: &HashMap<String, String>,
    ) -> String {
        let mut result = template.to_string();

        for (key, value) in placeholders {
            let pattern = format!("{{{{{}}}}}", key);
            result = result.replace(&pattern, value);
        }

        result = Self::resolve_context_placeholders(&result, context);
        result = Self::resolve_conditionals(&result, context, placeholders);

        result
    }

    fn resolve_context_placeholders(template: &str, context: &SystemPromptContext) -> String {
        let mut result = template.to_string();

        result = result.replace("{{CWD}}", &context.cwd.display().to_string());
        result = result.replace("{{PLATFORM}}", &context.platform);
        result = result.replace("{{IDE}}", &context.ide);
        result = result.replace("{{MODEL_ID}}", &context.model_id);
        result = result.replace(
            "{{MODEL_FAMILY}}",
            &format!("{:?}", context.model_family()),
        );

        let now = std::time::SystemTime::now();
        let since_epoch = now.duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
        let days = since_epoch.as_secs() / 86400;
        let year = 1970 + (days / 365);
        result = result.replace("{{CURRENT_DATE}}", &format!("{}", year));

        for (key, value) in &context.runtime_placeholders {
            let pattern = format!("{{{{{}}}}}", key);
            let value_str = match value {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            result = result.replace(&pattern, &value_str);
        }

        result
    }

    fn resolve_conditionals(
        template: &str,
        context: &SystemPromptContext,
        placeholders: &HashMap<String, String>,
    ) -> String {
        let mut result = template.to_string();

        let patterns = [
            ("{{#if SUPPORTS_BROWSER}}", "{{/if}}", context.supports_browser),
            ("{{#if YOLO_MODE}}", "{{/if}}", context.yolo_mode),
            ("{{#if MCP_ENABLED}}", "{{/if}}", !context.mcp_servers.is_empty()),
            ("{{#if SUBAGENTS_ENABLED}}", "{{/if}}", context.subagents_enabled),
            ("{{#if MULTI_ROOT}}", "{{/if}}", context.multi_root_enabled),
        ];

        for (start_tag, end_tag, condition) in patterns {
            result = Self::process_conditional(&result, start_tag, end_tag, condition);
        }

        for (key, value) in placeholders {
            let start_tag = format!("{{{{#if {}}}}}", key);
            let end_tag = "{{/if}}";
            let condition = !value.is_empty() && value != "false";
            result = Self::process_conditional(&result, &start_tag, end_tag, condition);
        }

        result
    }

    fn process_conditional(template: &str, start_tag: &str, end_tag: &str, include: bool) -> String {
        let mut result = template.to_string();

        while let Some(start_pos) = result.find(start_tag) {
            let search_start = start_pos + start_tag.len();
            if let Some(relative_end) = result[search_start..].find(end_tag) {
                let end_pos = search_start + relative_end;
                let content = &result[search_start..end_pos];

                if include {
                    result = format!(
                        "{}{}{}",
                        &result[..start_pos],
                        content,
                        &result[end_pos + end_tag.len()..]
                    );
                } else {
                    result = format!("{}{}", &result[..start_pos], &result[end_pos + end_tag.len()..]);
                }
            } else {
                break;
            }
        }

        result
    }

    pub fn extract_placeholders(template: &str) -> Vec<String> {
        let mut placeholders = Vec::new();
        let mut chars = template.chars().peekable();

        while let Some(c) = chars.next() {
            if c == '{' {
                if let Some(&'{') = chars.peek() {
                    chars.next();
                    let mut name = String::new();

                    while let Some(&nc) = chars.peek() {
                        if nc == '}' {
                            chars.next();
                            if let Some(&'}') = chars.peek() {
                                chars.next();
                                if !name.is_empty()
                                    && !name.starts_with('#')
                                    && !name.starts_with('/')
                                    && !placeholders.contains(&name)
                                {
                                    placeholders.push(name);
                                }
                            }
                            break;
                        }
                        name.push(nc);
                        chars.next();
                    }
                }
            }
        }

        placeholders
    }

    pub fn validate_required(template: &str, required: &[&str]) -> Vec<String> {
        let present = Self::extract_placeholders(template);
        required
            .iter()
            .filter(|r| !present.contains(&r.to_string()))
            .map(|s| s.to_string())
            .collect()
    }

    pub fn get_nested_value<'a>(obj: &'a serde_json::Value, path: &str) -> Option<&'a serde_json::Value> {
        let mut current = obj;
        for key in path.split('.') {
            current = current.get(key)?;
        }
        Some(current)
    }

    pub fn escape(template: &str) -> String {
        template
            .replace("{{", "\\{\\{")
            .replace("}}", "\\}\\}")
    }

    pub fn unescape(template: &str) -> String {
        template
            .replace("\\{\\{", "{{")
            .replace("\\}\\}", "}}")
    }

    pub fn resolve_with_json(
        template: &str,
        context: &SystemPromptContext,
        placeholders: &serde_json::Value,
    ) -> String {
        let mut result = template.to_string();

        if let serde_json::Value::Object(map) = placeholders {
            for (key, value) in map {
                let pattern = format!("{{{{{}}}}}", key);
                let value_str = match value {
                    serde_json::Value::String(s) => s.clone(),
                    serde_json::Value::Bool(b) => b.to_string(),
                    serde_json::Value::Number(n) => n.to_string(),
                    serde_json::Value::Null => String::new(),
                    other => other.to_string(),
                };
                result = result.replace(&pattern, &value_str);
            }

            for (key, value) in map {
                if let serde_json::Value::Object(_) = value {
                    result = Self::resolve_nested_placeholders(&result, key, value);
                }
            }
        }

        result = Self::resolve_context_placeholders(&result, context);
        result
    }

    fn resolve_nested_placeholders(
        template: &str,
        prefix: &str,
        obj: &serde_json::Value,
    ) -> String {
        let mut result = template.to_string();

        if let serde_json::Value::Object(map) = obj {
            for (key, value) in map {
                let full_path = format!("{}.{}", prefix, key);
                let pattern = format!("{{{{{}}}}}", full_path);

                let value_str = match value {
                    serde_json::Value::String(s) => s.clone(),
                    serde_json::Value::Bool(b) => b.to_string(),
                    serde_json::Value::Number(n) => n.to_string(),
                    serde_json::Value::Null => String::new(),
                    other => other.to_string(),
                };

                result = result.replace(&pattern, &value_str);

                if let serde_json::Value::Object(_) = value {
                    result = Self::resolve_nested_placeholders(&result, &full_path, value);
                }
            }
        }

        result
    }
}

pub fn post_process_prompt(prompt: &str) -> String {
    let mut result = prompt.to_string();

    while result.contains("\n\n\n") {
        result = result.replace("\n\n\n", "\n\n");
    }

    result = result.trim().to_string();

    while result.ends_with("====") || result.ends_with("===") {
        result = result.trim_end_matches('=').trim().to_string();
    }

    result
}

#[derive(Debug, Clone)]
pub struct TemplateBuilder {
    sections: Vec<TemplateSection>,
}

#[derive(Debug, Clone)]
struct TemplateSection {
    name: String,
    content: String,
    priority: i32,
}

impl TemplateBuilder {
    pub fn new() -> Self {
        Self { sections: Vec::new() }
    }

    pub fn add_section(
        mut self,
        name: impl Into<String>,
        content: impl Into<String>,
        priority: i32,
    ) -> Self {
        self.sections.push(TemplateSection {
            name: name.into(),
            content: content.into(),
            priority,
        });
        self
    }

    pub fn build(mut self) -> String {
        self.sections.sort_by_key(|s| s.priority);

        let parts: Vec<&str> = self
            .sections
            .iter()
            .map(|s| s.content.as_str())
            .filter(|s| !s.is_empty())
            .collect();

        parts.join("\n\n")
    }
}

impl Default for TemplateBuilder {
    fn default() -> Self {
        Self::new()
    }
}

pub struct Template {
    content: String,
    placeholders: HashMap<String, String>,
}

impl Template {
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            placeholders: HashMap::new(),
        }
    }

    pub fn with_placeholder(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.placeholders.insert(key.into(), value.into());
        self
    }

    pub fn set(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.placeholders.insert(key.into(), value.into());
    }

    pub fn render(&self) -> String {
        let mut result = self.content.clone();
        for (key, value) in &self.placeholders {
            let pattern = format!("{{{{{}}}}}", key);
            result = result.replace(&pattern, value);
        }
        result
    }

    pub fn render_with_context(&self, context: &SystemPromptContext) -> String {
        let result = self.render();
        TemplateEngine::resolve(&result, context, &self.placeholders)
    }
}

impl Default for Template {
    fn default() -> Self {
        Self::new("")
    }
}
