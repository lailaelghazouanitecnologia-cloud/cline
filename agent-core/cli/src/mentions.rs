#![deny(clippy::all)]

use regex::Regex;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use tokio::fs;
use tokio::process::Command;

#[derive(Debug, Clone, PartialEq)]
pub enum MentionType {
    File(String),
    Folder(String),
    Url(String),
    Problems,
    Terminal,
    GitChanges,
    GitCommit(String),
}

#[derive(Debug, Clone)]
pub struct ParsedMention {
    pub mention_type: MentionType,
    pub original: String,
    pub display: String,
}

pub struct MentionParser {
    regex: Regex,
}

impl MentionParser {
    pub fn new() -> Self {
        let pattern = r"@(/[^\s,;:!?()]+|https?://[^\s,;:!?()]+|problems|terminal|git-changes|[a-f0-9]{7,40})(?:\s|$|[.,;:!?()])";
        Self {
            regex: Regex::new(pattern).unwrap(),
        }
    }

    pub fn find_mentions(&self, text: &str) -> Vec<ParsedMention> {
        let mut mentions = Vec::new();
        let mut seen = HashSet::new();

        for caps in self.regex.captures_iter(text) {
            if let Some(m) = caps.get(1) {
                let mention = m.as_str().to_string();

                if seen.contains(&mention) {
                    continue;
                }
                seen.insert(mention.clone());

                let (mention_type, display) = self.classify_mention(&mention);

                mentions.push(ParsedMention {
                    mention_type,
                    original: format!("@{}", mention),
                    display,
                });
            }
        }

        mentions
    }

    fn classify_mention(&self, mention: &str) -> (MentionType, String) {
        if mention.starts_with("http://") || mention.starts_with("https://") {
            (
                MentionType::Url(mention.to_string()),
                format!("'{}' (see below for content)", mention),
            )
        } else if mention == "problems" {
            (MentionType::Problems, "Workspace Problems".to_string())
        } else if mention == "terminal" {
            (MentionType::Terminal, "Terminal Output".to_string())
        } else if mention == "git-changes" {
            (MentionType::GitChanges, "Git Changes".to_string())
        } else if self.is_git_hash(mention) {
            (
                MentionType::GitCommit(mention.to_string()),
                format!("Git commit '{}'", mention),
            )
        } else if mention.ends_with('/') {
            (
                MentionType::Folder(mention.to_string()),
                format!("'{}' (see below for folder content)", mention),
            )
        } else {
            (
                MentionType::File(mention.to_string()),
                format!("'{}' (see below for file content)", mention),
            )
        }
    }

    fn is_git_hash(&self, s: &str) -> bool {
        s.len() >= 7 && s.len() <= 40 && s.chars().all(|c| c.is_ascii_hexdigit())
    }

    pub fn replace_mentions(&self, text: &str, mentions: &[ParsedMention]) -> String {
        let mut result = text.to_string();

        for mention in mentions {
            result = result.replace(&mention.original, &mention.display);
        }

        result
    }
}

impl Default for MentionParser {
    fn default() -> Self {
        Self::new()
    }
}

pub struct MentionResolver {
    working_dir: PathBuf,
}

impl MentionResolver {
    pub fn new(working_dir: PathBuf) -> Self {
        Self { working_dir }
    }

    pub async fn resolve(&self, mention: &ParsedMention) -> String {
        match &mention.mention_type {
            MentionType::File(path) => self.resolve_file(path).await,
            MentionType::Folder(path) => self.resolve_folder(path).await,
            MentionType::Url(url) => self.resolve_url(url).await,
            MentionType::Problems => self.resolve_problems().await,
            MentionType::Terminal => self.resolve_terminal().await,
            MentionType::GitChanges => self.resolve_git_changes().await,
            MentionType::GitCommit(hash) => self.resolve_git_commit(hash).await,
        }
    }

    async fn resolve_file(&self, rel_path: &str) -> String {
        let clean_path = rel_path.trim_start_matches('/');
        let abs_path = self.working_dir.join(clean_path);

        match fs::read_to_string(&abs_path).await {
            Ok(content) => {
                if Self::is_binary_content(&content) {
                    format!(
                        "<file_content path=\"{}\">\n(Binary file, unable to display content)\n</file_content>",
                        clean_path
                    )
                } else {
                    format!(
                        "<file_content path=\"{}\">\n{}\n</file_content>",
                        clean_path, content
                    )
                }
            }
            Err(e) => format!(
                "<file_content path=\"{}\">\nError reading file: {}\n</file_content>",
                clean_path, e
            ),
        }
    }

    async fn resolve_folder(&self, rel_path: &str) -> String {
        let clean_path = rel_path.trim_start_matches('/').trim_end_matches('/');
        let abs_path = self.working_dir.join(clean_path);

        match fs::read_dir(&abs_path).await {
            Ok(mut entries) => {
                let mut listing = String::new();
                let mut file_contents = Vec::new();

                while let Ok(Some(entry)) = entries.next_entry().await {
                    let name = entry.file_name().to_string_lossy().to_string();
                    let is_dir = entry.file_type().await.map(|t| t.is_dir()).unwrap_or(false);

                    if is_dir {
                        listing.push_str(&format!("├── {}/\n", name));
                    } else {
                        listing.push_str(&format!("├── {}\n", name));

                        let file_path = format!("{}/{}", clean_path, name);
                        let abs_file = abs_path.join(&name);

                        if let Ok(content) = fs::read_to_string(&abs_file).await {
                            if !Self::is_binary_content(&content) && content.len() < 50000 {
                                file_contents.push(format!(
                                    "<file_content path=\"{}\">\n{}\n</file_content>",
                                    file_path, content
                                ));
                            }
                        }
                    }
                }

                let mut result = format!(
                    "<folder_content path=\"{}\">\n{}\n",
                    clean_path, listing
                );

                for content in file_contents {
                    result.push_str(&content);
                    result.push_str("\n\n");
                }

                result.push_str("</folder_content>");
                result
            }
            Err(e) => format!(
                "<folder_content path=\"{}\">\nError reading folder: {}\n</folder_content>",
                clean_path, e
            ),
        }
    }

    async fn resolve_url(&self, url: &str) -> String {
        format!(
            "<url_content url=\"{}\">\n(URL fetching not available in CLI mode. Use a browser to view.)\n</url_content>",
            url
        )
    }

    async fn resolve_problems(&self) -> String {
        "<workspace_diagnostics>\n(Diagnostics not available in CLI mode.)\n</workspace_diagnostics>".to_string()
    }

    async fn resolve_terminal(&self) -> String {
        "<terminal_output>\n(Terminal output capture not available in CLI mode.)\n</terminal_output>".to_string()
    }

    async fn resolve_git_changes(&self) -> String {
        let output = Command::new("git")
            .args(["status", "--porcelain"])
            .current_dir(&self.working_dir)
            .output()
            .await;

        let status = match output {
            Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).to_string(),
            Ok(o) => format!("Error: {}", String::from_utf8_lossy(&o.stderr)),
            Err(e) => format!("Error running git: {}", e),
        };

        let diff_output = Command::new("git")
            .args(["diff", "--stat"])
            .current_dir(&self.working_dir)
            .output()
            .await;

        let diff = match diff_output {
            Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).to_string(),
            _ => String::new(),
        };

        format!(
            "<git_working_state>\nStatus:\n{}\n\nDiff summary:\n{}\n</git_working_state>",
            status, diff
        )
    }

    async fn resolve_git_commit(&self, hash: &str) -> String {
        let output = Command::new("git")
            .args(["show", "--stat", "--format=fuller", hash])
            .current_dir(&self.working_dir)
            .output()
            .await;

        match output {
            Ok(o) if o.status.success() => {
                let info = String::from_utf8_lossy(&o.stdout);
                format!(
                    "<git_commit hash=\"{}\">\n{}\n</git_commit>",
                    hash, info
                )
            }
            Ok(o) => format!(
                "<git_commit hash=\"{}\">\nError: {}\n</git_commit>",
                hash,
                String::from_utf8_lossy(&o.stderr)
            ),
            Err(e) => format!(
                "<git_commit hash=\"{}\">\nError running git: {}\n</git_commit>",
                hash, e
            ),
        }
    }

    fn is_binary_content(content: &str) -> bool {
        let sample: String = content.chars().take(8000).collect();
        let null_count = sample.chars().filter(|&c| c == '\0').count();
        null_count > 0 || sample.chars().any(|c| c.is_control() && c != '\n' && c != '\r' && c != '\t')
    }
}

pub async fn parse_and_resolve_mentions(
    text: &str,
    working_dir: &Path,
) -> String {
    let parser = MentionParser::new();
    let resolver = MentionResolver::new(working_dir.to_path_buf());

    let mentions = parser.find_mentions(text);

    if mentions.is_empty() {
        return text.to_string();
    }

    let mut result = parser.replace_mentions(text, &mentions);

    for mention in &mentions {
        if mention.original == "@/" {
            continue;
        }

        let resolved = resolver.resolve(mention).await;
        result.push_str("\n\n");
        result.push_str(&resolved);
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_mention() {
        let parser = MentionParser::new();
        let mentions = parser.find_mentions("Check @/src/main.rs for details");

        assert_eq!(mentions.len(), 1);
        assert!(matches!(mentions[0].mention_type, MentionType::File(_)));
    }

    #[test]
    fn test_folder_mention() {
        let parser = MentionParser::new();
        let mentions = parser.find_mentions("Look at @/src/ folder");

        assert_eq!(mentions.len(), 1);
        assert!(matches!(mentions[0].mention_type, MentionType::Folder(_)));
    }

    #[test]
    fn test_url_mention() {
        let parser = MentionParser::new();
        let mentions = parser.find_mentions("See @https://example.com/docs");

        assert_eq!(mentions.len(), 1);
        assert!(matches!(mentions[0].mention_type, MentionType::Url(_)));
    }

    #[test]
    fn test_git_commit() {
        let parser = MentionParser::new();
        let mentions = parser.find_mentions("Check commit @abc1234");

        assert_eq!(mentions.len(), 1);
        assert!(matches!(mentions[0].mention_type, MentionType::GitCommit(_)));
    }

    #[test]
    fn test_special_mentions() {
        let parser = MentionParser::new();

        let mentions = parser.find_mentions("@problems");
        assert!(matches!(mentions[0].mention_type, MentionType::Problems));

        let mentions = parser.find_mentions("@terminal");
        assert!(matches!(mentions[0].mention_type, MentionType::Terminal));

        let mentions = parser.find_mentions("@git-changes");
        assert!(matches!(mentions[0].mention_type, MentionType::GitChanges));
    }

    #[test]
    fn test_multiple_mentions() {
        let parser = MentionParser::new();
        let mentions = parser.find_mentions("Check @/src/main.rs and @/Cargo.toml");

        assert_eq!(mentions.len(), 2);
    }

    #[test]
    fn test_no_duplicates() {
        let parser = MentionParser::new();
        let mentions = parser.find_mentions("@/file.rs and again @/file.rs");

        assert_eq!(mentions.len(), 1);
    }
}
