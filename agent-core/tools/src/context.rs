use agent_config::Config;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Clone)]
pub struct ToolContext {
    config: Arc<Config>,
    working_directory: PathBuf,
    timeout_ms: u64,
}

impl ToolContext {
    pub fn new(config: Arc<Config>) -> Self {
        let working_directory = config.working_directory.clone();
        let timeout_ms = config.timeout_ms;

        Self {
            config,
            working_directory,
            timeout_ms,
        }
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    pub fn working_directory(&self) -> &PathBuf {
        &self.working_directory
    }

    pub fn timeout_ms(&self) -> u64 {
        self.timeout_ms
    }

    pub fn resolve_path(&self, path: &str) -> PathBuf {
        let path = PathBuf::from(path);
        if path.is_absolute() {
            return path;
        }
        self.working_directory.join(path)
    }

    pub fn with_working_directory(mut self, directory: PathBuf) -> Self {
        self.working_directory = directory;
        self
    }

    pub fn with_timeout(mut self, timeout_ms: u64) -> Self {
        self.timeout_ms = timeout_ms;
        self
    }
}
