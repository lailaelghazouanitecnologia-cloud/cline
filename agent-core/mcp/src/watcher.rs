#![deny(clippy::all)]
#![forbid(unsafe_code)]

use agent_common::{AgentError, AgentResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::{mpsc, RwLock};
use tokio::time::interval;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchedServerConfig {
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    pub env: HashMap<String, String>,
    pub disabled: bool,
    pub auto_approve: Vec<String>,
}

#[derive(Debug, Clone)]
pub enum ConfigChangeEvent {
    ServerAdded(String, WatchedServerConfig),
    ServerRemoved(String),
    ServerModified(String, WatchedServerConfig),
    ConfigReloaded,
}

pub struct McpConfigWatcher {
    config_paths: Vec<PathBuf>,
    last_modified: Arc<RwLock<HashMap<PathBuf, SystemTime>>>,
    servers: Arc<RwLock<HashMap<String, WatchedServerConfig>>>,
    event_sender: Option<mpsc::Sender<ConfigChangeEvent>>,
    poll_interval: Duration,
    running: Arc<RwLock<bool>>,
}

impl McpConfigWatcher {
    pub fn new() -> Self {
        Self {
            config_paths: Vec::new(),
            last_modified: Arc::new(RwLock::new(HashMap::new())),
            servers: Arc::new(RwLock::new(HashMap::new())),
            event_sender: None,
            poll_interval: Duration::from_secs(2),
            running: Arc::new(RwLock::new(false)),
        }
    }

    pub fn with_poll_interval(mut self, interval: Duration) -> Self {
        self.poll_interval = interval;
        self
    }

    pub fn add_config_path(&mut self, path: impl AsRef<Path>) {
        self.config_paths.push(path.as_ref().to_path_buf());
    }

    pub fn add_standard_paths(&mut self) {
        if let Some(home) = dirs::home_dir() {
            self.add_config_path(home.join(".config/cline/mcp_settings.json"));
            self.add_config_path(home.join(".cline/mcp_settings.json"));
        }

        if let Ok(cwd) = std::env::current_dir() {
            self.add_config_path(cwd.join(".cline/mcp_settings.json"));
            self.add_config_path(cwd.join("mcp_settings.json"));
        }
    }

    pub fn subscribe(&mut self) -> mpsc::Receiver<ConfigChangeEvent> {
        let (tx, rx) = mpsc::channel(100);
        self.event_sender = Some(tx);
        rx
    }

    pub async fn load_configs(&mut self) -> AgentResult<()> {
        let paths: Vec<_> = self.config_paths.clone();
        for path in paths {
            if path.exists() {
                self.load_config_file(&path).await?;
            }
        }
        Ok(())
    }

    async fn load_config_file(&mut self, path: &Path) -> AgentResult<()> {
        let content = tokio::fs::read_to_string(path)
            .await
            .map_err(|e| AgentError::io("read mcp config", e))?;

        let config: McpConfigFile = serde_json::from_str(&content)
            .map_err(|e| AgentError::deserialization(e.to_string()))?;

        let mut servers = self.servers.write().await;
        let mut last_modified = self.last_modified.write().await;

        if let Ok(meta) = tokio::fs::metadata(path).await {
            if let Ok(modified) = meta.modified() {
                last_modified.insert(path.to_path_buf(), modified);
            }
        }

        for (name, server_config) in config.mcp_servers {
            servers.insert(name, server_config);
        }

        Ok(())
    }

    pub async fn start_watching(&self) {
        let mut running = self.running.write().await;
        if *running {
            return;
        }
        *running = true;
        drop(running);

        let config_paths = self.config_paths.clone();
        let last_modified = self.last_modified.clone();
        let servers = self.servers.clone();
        let event_sender = self.event_sender.clone();
        let poll_interval = self.poll_interval;
        let running = self.running.clone();

        tokio::spawn(async move {
            let mut ticker = interval(poll_interval);

            loop {
                ticker.tick().await;

                let is_running = *running.read().await;
                if !is_running {
                    break;
                }

                for path in &config_paths {
                    if !path.exists() {
                        continue;
                    }

                    let current_modified = match tokio::fs::metadata(path).await {
                        Ok(meta) => meta.modified().ok(),
                        Err(_) => continue,
                    };

                    let last = {
                        let guard = last_modified.read().await;
                        guard.get(path).copied()
                    };

                    let needs_reload = match (last, current_modified) {
                        (Some(last), Some(current)) => current > last,
                        (None, Some(_)) => true,
                        _ => false,
                    };

                    if needs_reload {
                        if let Ok(content) = tokio::fs::read_to_string(path).await {
                            if let Ok(config) = serde_json::from_str::<McpConfigFile>(&content) {
                                let mut servers_guard = servers.write().await;
                                let mut modified_guard = last_modified.write().await;

                                if let Some(modified) = current_modified {
                                    modified_guard.insert(path.clone(), modified);
                                }

                                let old_servers: HashMap<_, _> = servers_guard.clone();

                                for (name, server_config) in config.mcp_servers {
                                    let is_new = !old_servers.contains_key(&name);
                                    let is_modified = old_servers
                                        .get(&name)
                                        .map(|old| !configs_equal(old, &server_config))
                                        .unwrap_or(false);

                                    if is_new {
                                        if let Some(ref tx) = event_sender {
                                            let _ = tx
                                                .send(ConfigChangeEvent::ServerAdded(
                                                    name.clone(),
                                                    server_config.clone(),
                                                ))
                                                .await;
                                        }
                                    } else if is_modified {
                                        if let Some(ref tx) = event_sender {
                                            let _ = tx
                                                .send(ConfigChangeEvent::ServerModified(
                                                    name.clone(),
                                                    server_config.clone(),
                                                ))
                                                .await;
                                        }
                                    }

                                    servers_guard.insert(name, server_config);
                                }

                                if let Some(ref tx) = event_sender {
                                    let _ = tx.send(ConfigChangeEvent::ConfigReloaded).await;
                                }
                            }
                        }
                    }
                }
            }
        });
    }

    pub async fn stop_watching(&self) {
        let mut running = self.running.write().await;
        *running = false;
    }

    pub async fn get_servers(&self) -> HashMap<String, WatchedServerConfig> {
        self.servers.read().await.clone()
    }

    pub async fn get_server(&self, name: &str) -> Option<WatchedServerConfig> {
        self.servers.read().await.get(name).cloned()
    }

    pub async fn is_running(&self) -> bool {
        *self.running.read().await
    }
}

impl Default for McpConfigWatcher {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct McpConfigFile {
    #[serde(rename = "mcpServers", default)]
    mcp_servers: HashMap<String, WatchedServerConfig>,
}

fn configs_equal(a: &WatchedServerConfig, b: &WatchedServerConfig) -> bool {
    a.command == b.command
        && a.args == b.args
        && a.env == b.env
        && a.disabled == b.disabled
        && a.auto_approve == b.auto_approve
}

pub struct ConfigReloader {
    watcher: Arc<RwLock<McpConfigWatcher>>,
    reload_callback: Option<Box<dyn Fn(ConfigChangeEvent) + Send + Sync>>,
}

impl ConfigReloader {
    pub fn new(watcher: McpConfigWatcher) -> Self {
        Self {
            watcher: Arc::new(RwLock::new(watcher)),
            reload_callback: None,
        }
    }

    pub fn on_reload<F>(mut self, callback: F) -> Self
    where
        F: Fn(ConfigChangeEvent) + Send + Sync + 'static,
    {
        self.reload_callback = Some(Box::new(callback));
        self
    }

    pub async fn start(&self) -> AgentResult<()> {
        let watcher = self.watcher.read().await;
        watcher.start_watching().await;
        Ok(())
    }

    pub async fn stop(&self) -> AgentResult<()> {
        let watcher = self.watcher.read().await;
        watcher.stop_watching().await;
        Ok(())
    }
}
