#![deny(clippy::all)]
#![forbid(unsafe_code)]

use agent_common::{AgentError, AgentResult};
use serde::{de::DeserializeOwned, Serialize};
use std::path::{Path, PathBuf};
use tokio::fs;

pub struct StorageManager {
    base_dir: PathBuf,
}

impl StorageManager {
    pub fn new(base_dir: impl AsRef<Path>) -> Self {
        Self {
            base_dir: base_dir.as_ref().to_path_buf(),
        }
    }

    pub fn default_path() -> AgentResult<PathBuf> {
        dirs::data_local_dir()
            .map(|p| p.join("agent"))
            .ok_or_else(|| AgentError::config("could not determine data directory"))
    }

    pub async fn ensure_dir(&self) -> AgentResult<()> {
        fs::create_dir_all(&self.base_dir)
            .await
            .map_err(|e| AgentError::io("create directory", e))
    }

    pub async fn write_json<T: Serialize>(&self, name: &str, data: &T) -> AgentResult<()> {
        self.ensure_dir().await?;

        let path = self.base_dir.join(format!("{}.json", name));
        let content = serde_json::to_string_pretty(data)
            .map_err(|e| AgentError::serialization(e.to_string()))?;

        fs::write(&path, content)
            .await
            .map_err(|e| AgentError::io("write file", e))
    }

    pub async fn read_json<T: DeserializeOwned>(&self, name: &str) -> AgentResult<Option<T>> {
        let path = self.base_dir.join(format!("{}.json", name));

        if !path.exists() {
            return Ok(None);
        }

        let content = fs::read_to_string(&path)
            .await
            .map_err(|e| AgentError::io("read file", e))?;

        let data = serde_json::from_str(&content)
            .map_err(|e| AgentError::deserialization(e.to_string()))?;

        Ok(Some(data))
    }

    pub async fn delete(&self, name: &str) -> AgentResult<()> {
        let path = self.base_dir.join(format!("{}.json", name));

        if path.exists() {
            fs::remove_file(&path)
                .await
                .map_err(|e| AgentError::io("delete file", e))?;
        }

        Ok(())
    }

    pub async fn list_files(&self, extension: &str) -> AgentResult<Vec<String>> {
        let mut files = Vec::new();

        if !self.base_dir.exists() {
            return Ok(files);
        }

        let mut entries = fs::read_dir(&self.base_dir)
            .await
            .map_err(|e| AgentError::io("read directory", e))?;

        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| AgentError::io("read entry", e))?
        {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some(extension) {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    files.push(stem.to_string());
                }
            }
        }

        Ok(files)
    }

    pub fn subdir(&self, name: &str) -> Self {
        Self {
            base_dir: self.base_dir.join(name),
        }
    }
}

impl Default for StorageManager {
    fn default() -> Self {
        Self::new(StorageManager::default_path().unwrap_or_else(|_| PathBuf::from(".agent")))
    }
}
