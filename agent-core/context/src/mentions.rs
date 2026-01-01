#![deny(clippy::all)]
#![forbid(unsafe_code)]

use agent_common::{AgentError, AgentResult};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::fs;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Mention {
    File { path: PathBuf },
    Folder { path: PathBuf },
    Url { url: String },
    Problems,
    Git { reference: String },
    Symbol { name: String, file: Option<PathBuf> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedMention {
    pub mention: Mention,
    pub content: Option<String>,
    pub error: Option<String>,
}

pub struct MentionParser {
    workspace: PathBuf,
}

impl MentionParser {
    pub fn new(workspace: impl AsRef<Path>) -> Self {
        Self {
            workspace: workspace.as_ref().to_path_buf(),
        }
    }

    pub fn parse(&self, input: &str) -> Vec<Mention> {
        let mut mentions = Vec::new();
        let mut chars = input.chars().peekable();
        let mut in_mention = false;
        let mut current = String::new();

        while let Some(ch) = chars.next() {
            if ch == '@' && !in_mention {
                in_mention = true;
                current.clear();
                continue;
            }

            if in_mention {
                if ch.is_whitespace() || ch == ',' || ch == ';' {
                    if let Some(mention) = self.parse_mention(&current) {
                        mentions.push(mention);
                    }
                    in_mention = false;
                    current.clear();
                } else {
                    current.push(ch);
                }
            }
        }

        if in_mention && !current.is_empty() {
            if let Some(mention) = self.parse_mention(&current) {
                mentions.push(mention);
            }
        }

        mentions
    }

    fn parse_mention(&self, text: &str) -> Option<Mention> {
        if text.eq_ignore_ascii_case("problems") {
            return Some(Mention::Problems);
        }

        if text.starts_with("http://") || text.starts_with("https://") {
            return Some(Mention::Url { url: text.to_string() });
        }

        if text.starts_with("git:") {
            return Some(Mention::Git {
                reference: text[4..].to_string(),
            });
        }

        if text.starts_with("symbol:") {
            let rest = &text[7..];
            let (name, file) = if let Some(pos) = rest.find(':') {
                (rest[..pos].to_string(), Some(PathBuf::from(&rest[pos + 1..])))
            } else {
                (rest.to_string(), None)
            };
            return Some(Mention::Symbol { name, file });
        }

        let path = if text.starts_with('/') || text.starts_with("./") || text.starts_with("../") {
            PathBuf::from(text)
        } else {
            self.workspace.join(text)
        };

        if text.ends_with('/') || self.looks_like_folder(text) {
            Some(Mention::Folder { path })
        } else {
            Some(Mention::File { path })
        }
    }

    fn looks_like_folder(&self, text: &str) -> bool {
        !text.contains('.') || text.ends_with('/')
    }

    pub async fn resolve(&self, mention: &Mention) -> ResolvedMention {
        match mention {
            Mention::File { path } => self.resolve_file(path).await,
            Mention::Folder { path } => self.resolve_folder(path).await,
            Mention::Url { url } => self.resolve_url(url).await,
            Mention::Problems => self.resolve_problems().await,
            Mention::Git { reference } => self.resolve_git(reference).await,
            Mention::Symbol { name, file } => self.resolve_symbol(name, file.as_deref()).await,
        }
    }

    async fn resolve_file(&self, path: &Path) -> ResolvedMention {
        let mention = Mention::File { path: path.to_path_buf() };

        match fs::read_to_string(path).await {
            Ok(content) => ResolvedMention {
                mention,
                content: Some(content),
                error: None,
            },
            Err(e) => ResolvedMention {
                mention,
                content: None,
                error: Some(format!("Failed to read file: {}", e)),
            },
        }
    }

    async fn resolve_folder(&self, path: &Path) -> ResolvedMention {
        let mention = Mention::Folder { path: path.to_path_buf() };

        match self.list_folder(path).await {
            Ok(listing) => ResolvedMention {
                mention,
                content: Some(listing),
                error: None,
            },
            Err(e) => ResolvedMention {
                mention,
                content: None,
                error: Some(e.to_string()),
            },
        }
    }

    async fn list_folder(&self, path: &Path) -> AgentResult<String> {
        let mut entries = fs::read_dir(path)
            .await
            .map_err(|e| AgentError::io("read folder", e))?;

        let mut items = Vec::new();

        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| AgentError::io("read entry", e))?
        {
            let name = entry.file_name().to_string_lossy().to_string();
            let meta = entry.metadata().await.ok();
            let suffix = if meta.map(|m| m.is_dir()).unwrap_or(false) {
                "/"
            } else {
                ""
            };
            items.push(format!("{}{}", name, suffix));
        }

        items.sort();
        Ok(items.join("\n"))
    }

    async fn resolve_url(&self, url: &str) -> ResolvedMention {
        ResolvedMention {
            mention: Mention::Url { url: url.to_string() },
            content: Some(format!("[URL reference: {}]", url)),
            error: None,
        }
    }

    async fn resolve_problems(&self) -> ResolvedMention {
        ResolvedMention {
            mention: Mention::Problems,
            content: Some("[Workspace problems will be provided by IDE integration]".to_string()),
            error: None,
        }
    }

    async fn resolve_git(&self, reference: &str) -> ResolvedMention {
        ResolvedMention {
            mention: Mention::Git {
                reference: reference.to_string(),
            },
            content: Some(format!("[Git reference: {}]", reference)),
            error: None,
        }
    }

    async fn resolve_symbol(&self, name: &str, file: Option<&Path>) -> ResolvedMention {
        ResolvedMention {
            mention: Mention::Symbol {
                name: name.to_string(),
                file: file.map(|p| p.to_path_buf()),
            },
            content: Some(format!(
                "[Symbol lookup: {} in {:?}]",
                name,
                file.map(|p| p.display().to_string())
            )),
            error: None,
        }
    }

    pub async fn resolve_all(&self, input: &str) -> Vec<ResolvedMention> {
        let mentions = self.parse(input);
        let mut resolved = Vec::with_capacity(mentions.len());

        for mention in mentions {
            resolved.push(self.resolve(&mention).await);
        }

        resolved
    }

    pub fn extract_and_clean(&self, input: &str) -> (String, Vec<Mention>) {
        let mentions = self.parse(input);
        let mut cleaned = input.to_string();

        for mention in &mentions {
            let pattern = self.mention_to_string(mention);
            cleaned = cleaned.replace(&format!("@{}", pattern), "");
        }

        cleaned = cleaned
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");

        (cleaned, mentions)
    }

    fn mention_to_string(&self, mention: &Mention) -> String {
        match mention {
            Mention::File { path } => path.display().to_string(),
            Mention::Folder { path } => path.display().to_string(),
            Mention::Url { url } => url.clone(),
            Mention::Problems => "problems".to_string(),
            Mention::Git { reference } => format!("git:{}", reference),
            Mention::Symbol { name, file } => {
                if let Some(f) = file {
                    format!("symbol:{}:{}", name, f.display())
                } else {
                    format!("symbol:{}", name)
                }
            }
        }
    }
}

#[derive(Debug, Default)]
pub struct MentionContext {
    pub files: Vec<(PathBuf, String)>,
    pub folders: Vec<(PathBuf, Vec<String>)>,
    pub urls: Vec<String>,
    pub has_problems: bool,
    pub git_refs: Vec<String>,
    pub symbols: Vec<(String, Option<PathBuf>)>,
}

impl MentionContext {
    pub fn from_resolved(resolved: Vec<ResolvedMention>) -> Self {
        let mut ctx = MentionContext::default();

        for r in resolved {
            if r.error.is_some() {
                continue;
            }

            match r.mention {
                Mention::File { path } => {
                    if let Some(content) = r.content {
                        ctx.files.push((path, content));
                    }
                }
                Mention::Folder { path } => {
                    if let Some(content) = r.content {
                        let items: Vec<String> = content.lines().map(|s| s.to_string()).collect();
                        ctx.folders.push((path, items));
                    }
                }
                Mention::Url { url } => {
                    ctx.urls.push(url);
                }
                Mention::Problems => {
                    ctx.has_problems = true;
                }
                Mention::Git { reference } => {
                    ctx.git_refs.push(reference);
                }
                Mention::Symbol { name, file } => {
                    ctx.symbols.push((name, file));
                }
            }
        }

        ctx
    }

    pub fn to_context_string(&self) -> String {
        let mut parts = Vec::new();

        for (path, content) in &self.files {
            parts.push(format!(
                "<file path=\"{}\">\n{}\n</file>",
                path.display(),
                content
            ));
        }

        for (path, items) in &self.folders {
            parts.push(format!(
                "<folder path=\"{}\">\n{}\n</folder>",
                path.display(),
                items.join("\n")
            ));
        }

        for url in &self.urls {
            parts.push(format!("<url>{}</url>", url));
        }

        if self.has_problems {
            parts.push("<problems>[Workspace diagnostics]</problems>".to_string());
        }

        for git_ref in &self.git_refs {
            parts.push(format!("<git ref=\"{}\" />", git_ref));
        }

        for (name, file) in &self.symbols {
            let file_attr = file
                .as_ref()
                .map(|f| format!(" file=\"{}\"", f.display()))
                .unwrap_or_default();
            parts.push(format!("<symbol name=\"{}\"{}/>", name, file_attr));
        }

        parts.join("\n\n")
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
            && self.folders.is_empty()
            && self.urls.is_empty()
            && !self.has_problems
            && self.git_refs.is_empty()
            && self.symbols.is_empty()
    }
}
