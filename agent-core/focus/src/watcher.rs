#![deny(clippy::all)]
#![forbid(unsafe_code)]

use crate::chain::FocusChain;
use crate::item::{FocusItem, FocusType};
use agent_common::{AgentError, AgentResult};
use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::fs;
use tokio::sync::{mpsc, RwLock};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoFile {
    pub path: PathBuf,
    pub items: Vec<TodoEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoEntry {
    pub line: usize,
    pub text: String,
    pub checked: bool,
}

pub struct FocusWatcher {
    chain: FocusChain,
    watch_paths: Arc<RwLock<Vec<PathBuf>>>,
    watcher: Option<RecommendedWatcher>,
    shutdown_tx: Option<mpsc::Sender<()>>,
}

impl FocusWatcher {
    pub fn new(chain: FocusChain) -> Self {
        Self {
            chain,
            watch_paths: Arc::new(RwLock::new(Vec::new())),
            watcher: None,
            shutdown_tx: None,
        }
    }

    pub async fn watch(&mut self, path: impl AsRef<Path>) -> AgentResult<()> {
        let path = path.as_ref().to_path_buf();

        if !path.exists() {
            return Err(AgentError::validation(format!(
                "Path does not exist: {}",
                path.display()
            )));
        }

        let mut paths = self.watch_paths.write().await;
        if !paths.contains(&path) {
            paths.push(path);
        }
        Ok(())
    }

    pub async fn start(&mut self) -> AgentResult<()> {
        let (tx, mut rx) = mpsc::channel::<Event>(100);
        let (shutdown_tx, mut shutdown_rx) = mpsc::channel::<()>(1);

        let event_handler = move |res: Result<Event, notify::Error>| {
            if let Ok(event) = res {
                let _ = tx.blocking_send(event);
            }
        };

        let mut watcher = RecommendedWatcher::new(event_handler, Config::default())
            .map_err(|e| AgentError::internal(format!("Failed to create watcher: {}", e)))?;

        let paths = self.watch_paths.read().await;
        for path in paths.iter() {
            watcher
                .watch(path, RecursiveMode::NonRecursive)
                .map_err(|e| AgentError::internal(format!("Failed to watch path: {}", e)))?;
        }

        self.watcher = Some(watcher);
        self.shutdown_tx = Some(shutdown_tx);

        let chain = self.chain.clone();
        let watch_paths = Arc::clone(&self.watch_paths);

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    Some(event) = rx.recv() => {
                        Self::handle_event(&chain, &watch_paths, event).await;
                    }
                    _ = shutdown_rx.recv() => {
                        break;
                    }
                }
            }
        });

        Ok(())
    }

    pub async fn stop(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(()).await;
        }
        self.watcher = None;
    }

    async fn handle_event(chain: &FocusChain, watch_paths: &RwLock<Vec<PathBuf>>, event: Event) {
        use notify::EventKind;

        match event.kind {
            EventKind::Modify(_) | EventKind::Create(_) => {
                for path in &event.paths {
                    let paths = watch_paths.read().await;
                    if paths.iter().any(|p| path.starts_with(p)) {
                        if let Ok(todos) = Self::parse_todo_file(path).await {
                            Self::sync_todos(chain, &todos).await;
                        }
                    }
                }
            }
            _ => {}
        }
    }

    async fn parse_todo_file(path: &Path) -> AgentResult<TodoFile> {
        let content = fs::read_to_string(path)
            .await
            .map_err(|e| AgentError::io("read todo file", e))?;

        let items = content
            .lines()
            .enumerate()
            .filter_map(|(line_num, line)| Self::parse_todo_line(line_num + 1, line))
            .collect();

        Ok(TodoFile {
            path: path.to_path_buf(),
            items,
        })
    }

    fn parse_todo_line(line: usize, text: &str) -> Option<TodoEntry> {
        let trimmed = text.trim();

        if trimmed.starts_with("- [ ]") {
            Some(TodoEntry {
                line,
                text: trimmed[5..].trim().to_string(),
                checked: false,
            })
        } else if trimmed.starts_with("- [x]") || trimmed.starts_with("- [X]") {
            Some(TodoEntry {
                line,
                text: trimmed[5..].trim().to_string(),
                checked: true,
            })
        } else if trimmed.starts_with("* [ ]") {
            Some(TodoEntry {
                line,
                text: trimmed[5..].trim().to_string(),
                checked: false,
            })
        } else if trimmed.starts_with("* [x]") || trimmed.starts_with("* [X]") {
            Some(TodoEntry {
                line,
                text: trimmed[5..].trim().to_string(),
                checked: true,
            })
        } else {
            None
        }
    }

    async fn sync_todos(chain: &FocusChain, todo_file: &TodoFile) {
        let file_prefix = todo_file
            .path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("todo");

        for entry in &todo_file.items {
            let id = format!("{}:{}", file_prefix, entry.line);

            if let Some(mut existing) = chain.get(&id).await {
                if entry.checked && !existing.is_done() {
                    existing.complete();
                    let _ = chain
                        .update(&id, crate::item::FocusStatus::Completed)
                        .await;
                }
            } else if !entry.checked {
                let item = FocusItem::new(&id, FocusType::Task, &entry.text)
                    .with_metadata(serde_json::json!({
                        "source": "file",
                        "file": todo_file.path.display().to_string(),
                        "line": entry.line
                    }));
                let _ = chain.add(item).await;
            }
        }
    }

    pub async fn scan_initial(&self) -> AgentResult<()> {
        let paths = self.watch_paths.read().await;
        for path in paths.iter() {
            if path.is_file() {
                if let Ok(todos) = Self::parse_todo_file(path).await {
                    Self::sync_todos(&self.chain, &todos).await;
                }
            } else if path.is_dir() {
                self.scan_directory(path).await?;
            }
        }
        Ok(())
    }

    async fn scan_directory(&self, dir: &Path) -> AgentResult<()> {
        let mut entries = fs::read_dir(dir)
            .await
            .map_err(|e| AgentError::io("read directory", e))?;

        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| AgentError::io("read entry", e))?
        {
            let path = entry.path();
            if path.is_file() {
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if name.to_lowercase().contains("todo")
                    || name.to_lowercase().contains("task")
                    || name.ends_with(".md")
                {
                    if let Ok(todos) = Self::parse_todo_file(&path).await {
                        Self::sync_todos(&self.chain, &todos).await;
                    }
                }
            }
        }
        Ok(())
    }
}
