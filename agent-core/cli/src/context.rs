#![deny(clippy::all)]

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileAccess {
    pub path: String,
    pub action: FileAction,
    pub timestamp: DateTime<Utc>,
    pub line_count: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum FileAction {
    Read,
    Write,
    Modify,
    Delete,
    Create,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolUsage {
    pub name: String,
    pub count: u32,
    pub last_used: DateTime<Utc>,
    pub avg_duration_ms: u64,
    pub errors: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
    pub total_tokens: u64,
}

impl Default for TokenUsage {
    fn default() -> Self {
        Self {
            input_tokens: 0,
            output_tokens: 0,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
            total_tokens: 0,
        }
    }
}

impl TokenUsage {
    pub fn add(&mut self, input: u64, output: u64) {
        self.input_tokens += input;
        self.output_tokens += output;
        self.total_tokens += input + output;
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionContext {
    pub session_id: String,
    pub working_directory: PathBuf,
    pub provider_id: String,
    pub model_id: String,
    pub started_at: DateTime<Utc>,
    pub file_accesses: Vec<FileAccess>,
    pub tool_usage: HashMap<String, ToolUsage>,
    pub token_usage: TokenUsage,
    pub turn_count: u32,
    pub environment: HashMap<String, String>,
}

impl SessionContext {
    pub fn new(session_id: &str, working_directory: PathBuf, provider_id: &str, model_id: &str) -> Self {
        Self {
            session_id: session_id.to_string(),
            working_directory,
            provider_id: provider_id.to_string(),
            model_id: model_id.to_string(),
            started_at: Utc::now(),
            file_accesses: Vec::new(),
            tool_usage: HashMap::new(),
            token_usage: TokenUsage::default(),
            turn_count: 0,
            environment: HashMap::new(),
        }
    }

    pub fn record_file_access(&mut self, path: &str, action: FileAction, line_count: Option<usize>) {
        self.file_accesses.push(FileAccess {
            path: path.to_string(),
            action,
            timestamp: Utc::now(),
            line_count,
        });
    }

    pub fn record_tool_use(&mut self, name: &str, duration_ms: u64, success: bool) {
        let entry = self.tool_usage.entry(name.to_string()).or_insert_with(|| ToolUsage {
            name: name.to_string(),
            count: 0,
            last_used: Utc::now(),
            avg_duration_ms: 0,
            errors: 0,
        });

        let total_duration = entry.avg_duration_ms * entry.count as u64 + duration_ms;
        entry.count += 1;
        entry.avg_duration_ms = total_duration / entry.count as u64;
        entry.last_used = Utc::now();

        if !success {
            entry.errors += 1;
        }
    }

    pub fn increment_turn(&mut self) {
        self.turn_count += 1;
    }

    pub fn files_modified(&self) -> Vec<&str> {
        self.file_accesses
            .iter()
            .filter(|f| matches!(f.action, FileAction::Write | FileAction::Modify | FileAction::Create))
            .map(|f| f.path.as_str())
            .collect()
    }

    pub fn files_read(&self) -> Vec<&str> {
        self.file_accesses
            .iter()
            .filter(|f| matches!(f.action, FileAction::Read))
            .map(|f| f.path.as_str())
            .collect()
    }

    pub fn to_summary(&self) -> ContextSummary {
        ContextSummary {
            session_id: self.session_id.clone(),
            working_directory: self.working_directory.display().to_string(),
            provider_id: self.provider_id.clone(),
            model_id: self.model_id.clone(),
            turn_count: self.turn_count,
            files_read: self.files_read().len(),
            files_modified: self.files_modified().len(),
            total_tokens: self.token_usage.total_tokens,
            duration_secs: (Utc::now() - self.started_at).num_seconds() as u64,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextSummary {
    pub session_id: String,
    pub working_directory: String,
    pub provider_id: String,
    pub model_id: String,
    pub turn_count: u32,
    pub files_read: usize,
    pub files_modified: usize,
    pub total_tokens: u64,
    pub duration_secs: u64,
}

pub struct ContextManager {
    contexts: Arc<RwLock<HashMap<String, SessionContext>>>,
}

impl ContextManager {
    pub fn new() -> Self {
        Self {
            contexts: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn create_context(
        &self,
        session_id: &str,
        working_directory: PathBuf,
        provider_id: &str,
        model_id: &str,
    ) -> SessionContext {
        let context = SessionContext::new(session_id, working_directory, provider_id, model_id);
        let mut contexts = self.contexts.write().unwrap();
        contexts.insert(session_id.to_string(), context.clone());
        context
    }

    pub fn get_context(&self, session_id: &str) -> Option<SessionContext> {
        let contexts = self.contexts.read().unwrap();
        contexts.get(session_id).cloned()
    }

    pub fn update_context<F>(&self, session_id: &str, f: F)
    where
        F: FnOnce(&mut SessionContext),
    {
        let mut contexts = self.contexts.write().unwrap();
        if let Some(context) = contexts.get_mut(session_id) {
            f(context);
        }
    }

    pub fn remove_context(&self, session_id: &str) {
        let mut contexts = self.contexts.write().unwrap();
        contexts.remove(session_id);
    }

    pub fn list_active(&self) -> Vec<ContextSummary> {
        let contexts = self.contexts.read().unwrap();
        contexts.values().map(|c| c.to_summary()).collect()
    }
}

impl Clone for ContextManager {
    fn clone(&self) -> Self {
        Self {
            contexts: Arc::clone(&self.contexts),
        }
    }
}

impl Default for ContextManager {
    fn default() -> Self {
        Self::new()
    }
}
