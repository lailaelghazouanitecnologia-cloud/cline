#![deny(clippy::all)]
#![forbid(unsafe_code)]

use crate::components::{get_all_components, ComponentMapping};
use crate::template::{post_process_prompt, TemplateEngine};
use crate::types::{ModelFamily, SystemPromptContext};
use crate::variant::{get_all_variants, PromptVariant};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct PromptRegistry {
    variants: Vec<PromptVariant>,
    components: Vec<ComponentMapping>,
    loaded: bool,
}

impl PromptRegistry {
    pub fn new() -> Self {
        Self {
            variants: Vec::new(),
            components: Vec::new(),
            loaded: false,
        }
    }

    pub async fn load(&mut self) {
        if self.loaded {
            return;
        }

        self.variants = get_all_variants();
        self.components = get_all_components();

        self.perform_health_check();
        self.loaded = true;
    }

    fn perform_health_check(&self) {
        let has_generic = self.variants.iter().any(|v| v.family == ModelFamily::Generic);

        if !has_generic {
            tracing::error!("Registry health check failed: Missing generic variant");
        }

        if self.variants.is_empty() {
            tracing::error!("Registry health check failed: No variants loaded");
        }

        if self.components.is_empty() {
            tracing::warn!("Registry health check warning: No components loaded");
        }

        tracing::info!(
            "Registry health check: {} variants, {} components loaded",
            self.variants.len(),
            self.components.len()
        );
    }

    pub fn get_model_family(&self, context: &SystemPromptContext) -> ModelFamily {
        for variant in &self.variants {
            if variant.matches(context) {
                return variant.family;
            }
        }
        ModelFamily::Generic
    }

    pub async fn get(&self, context: &SystemPromptContext) -> Result<String, PromptError> {
        let family = self.get_model_family(context);

        let variant = self
            .variants
            .iter()
            .find(|v| v.family == family)
            .ok_or_else(|| PromptError::VariantNotFound(format!("{:?}", family)))?;

        self.build_prompt(variant, context).await
    }

    pub async fn get_by_family(
        &self,
        family: ModelFamily,
        context: &SystemPromptContext,
    ) -> Result<String, PromptError> {
        let variant = self
            .variants
            .iter()
            .find(|v| v.family == family)
            .ok_or_else(|| PromptError::VariantNotFound(format!("{:?}", family)))?;

        self.build_prompt(variant, context).await
    }

    async fn build_prompt(
        &self,
        variant: &PromptVariant,
        context: &SystemPromptContext,
    ) -> Result<String, PromptError> {
        let component_sections = self.build_components(variant, context).await;
        let placeholders = self.prepare_placeholders(variant, context, &component_sections);

        let prompt = TemplateEngine::resolve(&variant.base_template, context, &placeholders);
        let processed = post_process_prompt(&prompt);

        Ok(processed)
    }

    async fn build_components(
        &self,
        variant: &PromptVariant,
        context: &SystemPromptContext,
    ) -> HashMap<String, String> {
        let mut sections = HashMap::new();

        for section in &variant.component_order {
            if !variant.is_component_enabled(*section) {
                continue;
            }

            if let Some(component) = self.components.iter().find(|c| c.id == *section) {
                let future = (component.func)(variant, context);
                if let Some(content) = future.await {
                    if !content.trim().is_empty() {
                        sections.insert(section.as_str().to_uppercase(), content);
                    }
                }
            }
        }

        sections
    }

    fn prepare_placeholders(
        &self,
        variant: &PromptVariant,
        context: &SystemPromptContext,
        component_sections: &HashMap<String, String>,
    ) -> HashMap<String, String> {
        let mut placeholders = HashMap::new();

        for (key, value) in &variant.placeholders {
            placeholders.insert(key.clone(), value.clone());
        }

        placeholders.insert("CWD".to_string(), context.cwd.display().to_string());
        placeholders.insert(
            "SUPPORTS_BROWSER".to_string(),
            context.supports_browser.to_string(),
        );
        placeholders.insert("MODEL_FAMILY".to_string(), format!("{:?}", variant.family));

        for (key, value) in component_sections {
            placeholders.insert(key.clone(), value.clone());
        }

        for (key, value) in &context.runtime_placeholders {
            let value_str = match value {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            placeholders.insert(key.clone(), value_str);
        }

        placeholders
    }

    pub fn get_variant_metadata(&self, id: &str) -> Option<&PromptVariant> {
        self.variants.iter().find(|v| v.id == id)
    }

    pub fn get_available_models(&self) -> Vec<String> {
        self.variants.iter().map(|v| v.id.clone()).collect()
    }

    pub fn register_variant(&mut self, variant: PromptVariant) {
        self.variants.insert(0, variant);
    }
}

impl Default for PromptRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub enum PromptError {
    VariantNotFound(String),
    ComponentError(String),
    TemplateError(String),
}

impl std::fmt::Display for PromptError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::VariantNotFound(msg) => write!(f, "Variant not found: {}", msg),
            Self::ComponentError(msg) => write!(f, "Component error: {}", msg),
            Self::TemplateError(msg) => write!(f, "Template error: {}", msg),
        }
    }
}

impl std::error::Error for PromptError {}

pub struct SharedRegistry {
    inner: Arc<RwLock<PromptRegistry>>,
}

impl SharedRegistry {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(PromptRegistry::new())),
        }
    }

    pub async fn load(&self) {
        self.inner.write().await.load().await;
    }

    pub async fn get(&self, context: &SystemPromptContext) -> Result<String, PromptError> {
        self.inner.read().await.get(context).await
    }

    pub async fn get_model_family(&self, context: &SystemPromptContext) -> ModelFamily {
        self.inner.read().await.get_model_family(context)
    }
}

impl Default for SharedRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for SharedRegistry {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}
