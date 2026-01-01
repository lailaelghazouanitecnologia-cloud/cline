#![deny(clippy::all)]
#![forbid(unsafe_code)]

use crate::types::{HookDefinition, HookEvent};
use agent_common::{AgentError, AgentResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;
use tokio::fs;
use tokio::sync::RwLock;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct HooksConfigFile {
    #[serde(default)]
    hooks: Vec<HookConfigEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct HookConfigEntry {
    name: String,
    event: String,
    command: String,
    #[serde(default)]
    enabled: Option<bool>,
    #[serde(default)]
    timeout_ms: Option<u64>,
    #[serde(default)]
    fail_on_error: Option<bool>,
    #[serde(default)]
    env: Option<HashMap<String, String>>,
}

pub struct HookLoader {
    search_paths: Vec<PathBuf>,
    cache: Arc<RwLock<HookCache>>,
    cache_ttl_secs: u64,
}

struct HookCache {
    hooks: Vec<HookDefinition>,
    last_loaded: Option<SystemTime>,
    file_timestamps: HashMap<PathBuf, SystemTime>,
}

impl HookLoader {
    pub fn new() -> Self {
        Self {
            search_paths: Vec::new(),
            cache: Arc::new(RwLock::new(HookCache {
                hooks: Vec::new(),
                last_loaded: None,
                file_timestamps: HashMap::new(),
            })),
            cache_ttl_secs: 60,
        }
    }

    pub fn with_ttl(mut self, secs: u64) -> Self {
        self.cache_ttl_secs = secs;
        self
    }

    pub fn add_search_path(&mut self, path: impl AsRef<Path>) {
        self.search_paths.push(path.as_ref().to_path_buf());
    }

    pub fn add_standard_paths(&mut self, workspace: &Path) {
        self.add_search_path(workspace.join(".agent/hooks.json"));
        self.add_search_path(workspace.join(".cline/hooks.json"));

        if let Some(home) = dirs::home_dir() {
            self.add_search_path(home.join(".config/cline/hooks.json"));
            self.add_search_path(home.join(".agent/hooks.json"));
        }
    }

    pub async fn discover(&self) -> AgentResult<Vec<HookDefinition>> {
        if self.is_cache_valid().await {
            let cache = self.cache.read().await;
            return Ok(cache.hooks.clone());
        }

        let mut all_hooks = Vec::new();
        let mut timestamps = HashMap::new();

        for path in &self.search_paths {
            if path.exists() {
                if let Ok(hooks) = self.load_from_file(path).await {
                    all_hooks.extend(hooks);

                    if let Ok(meta) = fs::metadata(path).await {
                        if let Ok(modified) = meta.modified() {
                            timestamps.insert(path.clone(), modified);
                        }
                    }
                }
            }
        }

        let mut cache = self.cache.write().await;
        cache.hooks = all_hooks.clone();
        cache.last_loaded = Some(SystemTime::now());
        cache.file_timestamps = timestamps;

        Ok(all_hooks)
    }

    pub async fn discover_for_event(&self, event: HookEvent) -> AgentResult<Vec<HookDefinition>> {
        let all_hooks = self.discover().await?;
        Ok(all_hooks
            .into_iter()
            .filter(|h| h.event == event && h.enabled)
            .collect())
    }

    async fn is_cache_valid(&self) -> bool {
        let cache = self.cache.read().await;

        let last_loaded = match cache.last_loaded {
            Some(t) => t,
            None => return false,
        };

        let elapsed = SystemTime::now()
            .duration_since(last_loaded)
            .map(|d| d.as_secs())
            .unwrap_or(u64::MAX);

        if elapsed > self.cache_ttl_secs {
            return false;
        }

        for (path, cached_time) in &cache.file_timestamps {
            if let Ok(meta) = fs::metadata(path).await {
                if let Ok(current_time) = meta.modified() {
                    if current_time > *cached_time {
                        return false;
                    }
                }
            }
        }

        true
    }

    async fn load_from_file(&self, path: &Path) -> AgentResult<Vec<HookDefinition>> {
        let content = fs::read_to_string(path)
            .await
            .map_err(|e| AgentError::io("read hooks file", e))?;

        let config: HooksConfigFile = serde_json::from_str(&content)
            .map_err(|e| AgentError::deserialization(e.to_string()))?;

        let mut hooks = Vec::new();

        for entry in config.hooks {
            if let Some(hook) = self.parse_entry(&entry) {
                hooks.push(hook);
            }
        }

        Ok(hooks)
    }

    fn parse_entry(&self, entry: &HookConfigEntry) -> Option<HookDefinition> {
        let event = match entry.event.as_str() {
            "session_start" => HookEvent::SessionStart,
            "session_end" => HookEvent::SessionEnd,
            "before_tool_call" => HookEvent::BeforeToolCall,
            "after_tool_call" => HookEvent::AfterToolCall,
            "before_message" => HookEvent::BeforeMessage,
            "after_message" => HookEvent::AfterMessage,
            "on_error" => HookEvent::OnError,
            "user_prompt_submit" => HookEvent::UserPromptSubmit,
            _ => return None,
        };

        let mut hook = HookDefinition::new(&entry.name, event, &entry.command);

        if let Some(enabled) = entry.enabled {
            if !enabled {
                hook = hook.disabled();
            }
        }

        if let Some(timeout) = entry.timeout_ms {
            hook = hook.with_timeout(timeout);
        }

        if let Some(fail_on_error) = entry.fail_on_error {
            hook = hook.with_fail_on_error(fail_on_error);
        }

        if let Some(ref env) = entry.env {
            for (key, value) in env {
                hook = hook.with_env(key, value);
            }
        }

        Some(hook)
    }

    pub async fn invalidate_cache(&self) {
        let mut cache = self.cache.write().await;
        cache.last_loaded = None;
    }

    pub async fn reload(&self) -> AgentResult<Vec<HookDefinition>> {
        self.invalidate_cache().await;
        self.discover().await
    }
}

impl Default for HookLoader {
    fn default() -> Self {
        Self::new()
    }
}

pub struct LifecycleHooks {
    loader: HookLoader,
}

impl LifecycleHooks {
    pub fn new(workspace: &Path) -> Self {
        let mut loader = HookLoader::new();
        loader.add_standard_paths(workspace);

        Self { loader }
    }

    pub async fn get_task_start_hooks(&self) -> AgentResult<Vec<HookDefinition>> {
        self.loader.discover_for_event(HookEvent::SessionStart).await
    }

    pub async fn get_task_end_hooks(&self) -> AgentResult<Vec<HookDefinition>> {
        self.loader.discover_for_event(HookEvent::SessionEnd).await
    }

    pub async fn get_pre_tool_hooks(&self) -> AgentResult<Vec<HookDefinition>> {
        self.loader.discover_for_event(HookEvent::BeforeToolCall).await
    }

    pub async fn get_post_tool_hooks(&self) -> AgentResult<Vec<HookDefinition>> {
        self.loader.discover_for_event(HookEvent::AfterToolCall).await
    }

    pub async fn get_error_hooks(&self) -> AgentResult<Vec<HookDefinition>> {
        self.loader.discover_for_event(HookEvent::OnError).await
    }

    pub async fn reload(&self) -> AgentResult<()> {
        self.loader.reload().await?;
        Ok(())
    }
}
