use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub name: String,
    pub base_url: String,
    pub api_key_env: Option<String>,
    pub default_model: String,
}

impl ProviderConfig {
    pub fn new(name: impl Into<String>, base_url: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            base_url: base_url.into(),
            api_key_env: None,
            default_model: String::new(),
        }
    }

    pub fn with_api_key_env(mut self, env_var: impl Into<String>) -> Self {
        self.api_key_env = Some(env_var.into());
        self
    }

    pub fn with_default_model(mut self, model: impl Into<String>) -> Self {
        self.default_model = model.into();
        self
    }

    pub fn api_key(&self) -> Option<String> {
        self.api_key_env
            .as_ref()
            .and_then(|env_var| std::env::var(env_var).ok())
    }

    pub fn has_api_key(&self) -> bool {
        self.api_key().is_some()
    }
}
