#![deny(clippy::all)]
#![forbid(unsafe_code)]

use crate::tokenizer::count_tokens;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct FileContext {
    pub path: PathBuf,
    pub content: String,
    pub token_count: usize,
    pub last_accessed: u64,
    pub access_count: u32,
    pub modified: bool,
}

impl FileContext {
    pub fn new(path: impl Into<PathBuf>, content: impl Into<String>) -> Self {
        let content = content.into();
        let token_count = count_tokens(&content);
        Self {
            path: path.into(),
            content,
            token_count,
            last_accessed: 0,
            access_count: 1,
            modified: false,
        }
    }

    pub fn update(&mut self, content: impl Into<String>, timestamp: u64) {
        self.content = content.into();
        self.token_count = count_tokens(&self.content);
        self.last_accessed = timestamp;
        self.access_count += 1;
        self.modified = true;
    }

    pub fn mark_accessed(&mut self, timestamp: u64) {
        self.last_accessed = timestamp;
        self.access_count += 1;
    }
}

pub struct FileContextTracker {
    files: HashMap<PathBuf, FileContext>,
    total_tokens: usize,
    max_tokens: usize,
    access_counter: u64,
}

impl FileContextTracker {
    pub fn new(max_tokens: usize) -> Self {
        Self {
            files: HashMap::new(),
            total_tokens: 0,
            max_tokens,
            access_counter: 0,
        }
    }

    pub fn add_file(&mut self, path: impl Into<PathBuf>, content: impl Into<String>) {
        let path = path.into();
        let content = content.into();
        self.access_counter += 1;

        if let Some(existing) = self.files.get_mut(&path) {
            self.total_tokens -= existing.token_count;
            existing.update(content, self.access_counter);
            self.total_tokens += existing.token_count;
        } else {
            let ctx = FileContext::new(path.clone(), content);
            self.total_tokens += ctx.token_count;
            self.files.insert(path, ctx);
        }

        self.evict_if_needed();
    }

    pub fn get_file(&mut self, path: &PathBuf) -> Option<&FileContext> {
        self.access_counter += 1;
        if let Some(ctx) = self.files.get_mut(path) {
            ctx.mark_accessed(self.access_counter);
        }
        self.files.get(path)
    }

    pub fn remove_file(&mut self, path: &PathBuf) -> Option<FileContext> {
        if let Some(ctx) = self.files.remove(path) {
            self.total_tokens -= ctx.token_count;
            Some(ctx)
        } else {
            None
        }
    }

    pub fn total_tokens(&self) -> usize {
        self.total_tokens
    }

    pub fn file_count(&self) -> usize {
        self.files.len()
    }

    pub fn files(&self) -> impl Iterator<Item = &FileContext> {
        self.files.values()
    }

    pub fn modified_files(&self) -> impl Iterator<Item = &FileContext> {
        self.files.values().filter(|f| f.modified)
    }

    pub fn clear(&mut self) {
        self.files.clear();
        self.total_tokens = 0;
    }

    fn evict_if_needed(&mut self) {
        while self.total_tokens > self.max_tokens && self.files.len() > 1 {
            let lru_path = self.find_lru_file();
            if let Some(path) = lru_path {
                self.remove_file(&path);
            } else {
                break;
            }
        }
    }

    fn find_lru_file(&self) -> Option<PathBuf> {
        self.files
            .iter()
            .filter(|(_, ctx)| !ctx.modified)
            .min_by_key(|(_, ctx)| ctx.last_accessed)
            .map(|(path, _)| path.clone())
    }
}

impl Default for FileContextTracker {
    fn default() -> Self {
        Self::new(50_000)
    }
}

pub struct MentionTracker {
    mentions: HashMap<String, MentionInfo>,
}

#[derive(Debug, Clone)]
pub struct MentionInfo {
    pub mention_type: MentionType,
    pub value: String,
    pub resolved: bool,
    pub token_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MentionType {
    File,
    Folder,
    Url,
    Symbol,
    Problems,
    Git,
}

impl MentionTracker {
    pub fn new() -> Self {
        Self {
            mentions: HashMap::new(),
        }
    }

    pub fn add(&mut self, key: impl Into<String>, mention_type: MentionType, value: impl Into<String>) {
        let value = value.into();
        let token_count = count_tokens(&value);
        self.mentions.insert(
            key.into(),
            MentionInfo {
                mention_type,
                value,
                resolved: true,
                token_count,
            },
        );
    }

    pub fn get(&self, key: &str) -> Option<&MentionInfo> {
        self.mentions.get(key)
    }

    pub fn total_tokens(&self) -> usize {
        self.mentions.values().map(|m| m.token_count).sum()
    }

    pub fn by_type(&self, mention_type: MentionType) -> impl Iterator<Item = &MentionInfo> {
        self.mentions
            .values()
            .filter(move |m| m.mention_type == mention_type)
    }

    pub fn clear(&mut self) {
        self.mentions.clear();
    }
}

impl Default for MentionTracker {
    fn default() -> Self {
        Self::new()
    }
}
