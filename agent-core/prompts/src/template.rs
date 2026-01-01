#![deny(clippy::all)]
#![forbid(unsafe_code)]

use std::collections::HashMap;

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
        self.placeholders.insert(format!("{{{{{}}}}}", key.into()), value.into());
        self
    }

    pub fn set(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.placeholders.insert(format!("{{{{{}}}}}", key.into()), value.into());
    }

    pub fn render(&self) -> String {
        let mut result = self.content.clone();
        for (placeholder, value) in &self.placeholders {
            result = result.replace(placeholder, value);
        }
        result
    }

    pub fn extend(&mut self, other: &Template) {
        for (key, value) in &other.placeholders {
            self.placeholders.insert(key.clone(), value.clone());
        }
    }
}

impl Default for Template {
    fn default() -> Self {
        Self::new("")
    }
}

pub struct TemplateEngine {
    templates: HashMap<String, Template>,
}

impl TemplateEngine {
    pub fn new() -> Self {
        Self {
            templates: HashMap::new(),
        }
    }

    pub fn register(&mut self, name: impl Into<String>, template: Template) {
        self.templates.insert(name.into(), template);
    }

    pub fn get(&self, name: &str) -> Option<&Template> {
        self.templates.get(name)
    }

    pub fn render(&self, name: &str, context: &HashMap<String, String>) -> Option<String> {
        self.templates.get(name).map(|template| {
            let mut result = template.render();
            for (key, value) in context {
                result = result.replace(&format!("{{{{{}}}}}", key), value);
            }
            result
        })
    }
}

impl Default for TemplateEngine {
    fn default() -> Self {
        Self::new()
    }
}
