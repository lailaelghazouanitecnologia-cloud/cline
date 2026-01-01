#![deny(clippy::all)]
#![forbid(unsafe_code)]

use agent_common::{AgentError, AgentResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::fs;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleSet {
    pub source: RuleSource,
    pub rules: Vec<Rule>,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleSource {
    ClineruleFile { path: PathBuf },
    ClineruleDir { path: PathBuf },
    CursorRules { path: PathBuf },
    WindsurfRules { path: PathBuf },
    AgentRules { path: PathBuf },
    Custom { name: String },
    Remote { url: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rule {
    pub id: String,
    pub content: String,
    pub priority: u8,
    pub enabled: bool,
    pub tags: Vec<String>,
}

impl Rule {
    pub fn new(id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            content: content.into(),
            priority: 5,
            enabled: true,
            tags: Vec::new(),
        }
    }

    pub fn with_priority(mut self, priority: u8) -> Self {
        self.priority = priority.min(10);
        self
    }

    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }

    pub fn disable(mut self) -> Self {
        self.enabled = false;
        self
    }
}

pub struct RulesManager {
    workspace: PathBuf,
    rule_sets: HashMap<String, RuleSet>,
    toggles: HashMap<String, bool>,
}

impl RulesManager {
    pub fn new(workspace: impl AsRef<Path>) -> Self {
        Self {
            workspace: workspace.as_ref().to_path_buf(),
            rule_sets: HashMap::new(),
            toggles: HashMap::new(),
        }
    }

    pub async fn load(&mut self) -> AgentResult<()> {
        self.load_clinerules().await?;
        self.load_cursor_rules().await?;
        self.load_windsurf_rules().await?;
        self.load_agent_rules().await?;
        Ok(())
    }

    async fn load_clinerules(&mut self) -> AgentResult<()> {
        let clinerules_file = self.workspace.join(".clinerules");
        if clinerules_file.exists() {
            let content = fs::read_to_string(&clinerules_file)
                .await
                .map_err(|e| AgentError::io("read .clinerules", e))?;

            let rules = self.parse_rules_content(&content, "clinerules");
            self.rule_sets.insert(
                "clinerules".to_string(),
                RuleSet {
                    source: RuleSource::ClineruleFile {
                        path: clinerules_file,
                    },
                    rules,
                    enabled: true,
                },
            );
        }

        let clinerules_dir = self.workspace.join(".clinerules");
        if clinerules_dir.is_dir() {
            let rules = self.load_rules_from_dir(&clinerules_dir).await?;
            self.rule_sets.insert(
                "clinerules_dir".to_string(),
                RuleSet {
                    source: RuleSource::ClineruleDir {
                        path: clinerules_dir,
                    },
                    rules,
                    enabled: true,
                },
            );
        }

        Ok(())
    }

    async fn load_cursor_rules(&mut self) -> AgentResult<()> {
        let cursor_rules = self.workspace.join(".cursorrules");
        if cursor_rules.exists() {
            let content = fs::read_to_string(&cursor_rules)
                .await
                .map_err(|e| AgentError::io("read .cursorrules", e))?;

            let rules = self.parse_rules_content(&content, "cursor");
            self.rule_sets.insert(
                "cursor".to_string(),
                RuleSet {
                    source: RuleSource::CursorRules { path: cursor_rules },
                    rules,
                    enabled: true,
                },
            );
        }
        Ok(())
    }

    async fn load_windsurf_rules(&mut self) -> AgentResult<()> {
        let windsurf_rules = self.workspace.join(".windsurfrules");
        if windsurf_rules.exists() {
            let content = fs::read_to_string(&windsurf_rules)
                .await
                .map_err(|e| AgentError::io("read .windsurfrules", e))?;

            let rules = self.parse_rules_content(&content, "windsurf");
            self.rule_sets.insert(
                "windsurf".to_string(),
                RuleSet {
                    source: RuleSource::WindsurfRules {
                        path: windsurf_rules,
                    },
                    rules,
                    enabled: true,
                },
            );
        }
        Ok(())
    }

    async fn load_agent_rules(&mut self) -> AgentResult<()> {
        let agent_rules = self.workspace.join(".agentrules");
        if agent_rules.exists() {
            let content = fs::read_to_string(&agent_rules)
                .await
                .map_err(|e| AgentError::io("read .agentrules", e))?;

            let rules = self.parse_rules_content(&content, "agent");
            self.rule_sets.insert(
                "agent".to_string(),
                RuleSet {
                    source: RuleSource::AgentRules { path: agent_rules },
                    rules,
                    enabled: true,
                },
            );
        }
        Ok(())
    }

    async fn load_rules_from_dir(&self, dir: &Path) -> AgentResult<Vec<Rule>> {
        let mut rules = Vec::new();
        let mut entries = fs::read_dir(dir)
            .await
            .map_err(|e| AgentError::io("read rules dir", e))?;

        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| AgentError::io("read entry", e))?
        {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("md") {
                let content = fs::read_to_string(&path)
                    .await
                    .map_err(|e| AgentError::io("read rule file", e))?;

                let id = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("unknown");

                rules.push(Rule::new(id, content));
            }
        }

        rules.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(rules)
    }

    fn parse_rules_content(&self, content: &str, prefix: &str) -> Vec<Rule> {
        let sections = self.split_into_sections(content);
        sections
            .into_iter()
            .enumerate()
            .map(|(i, section)| Rule::new(format!("{}_{}", prefix, i + 1), section))
            .collect()
    }

    fn split_into_sections(&self, content: &str) -> Vec<String> {
        let mut sections = Vec::new();
        let mut current = String::new();

        for line in content.lines() {
            if line.starts_with("# ") && !current.is_empty() {
                sections.push(current.trim().to_string());
                current = String::new();
            }
            current.push_str(line);
            current.push('\n');
        }

        if !current.trim().is_empty() {
            sections.push(current.trim().to_string());
        }

        if sections.is_empty() && !content.trim().is_empty() {
            sections.push(content.trim().to_string());
        }

        sections
    }

    pub fn toggle(&mut self, rule_set_id: &str, enabled: bool) {
        if let Some(rule_set) = self.rule_sets.get_mut(rule_set_id) {
            rule_set.enabled = enabled;
        }
        self.toggles.insert(rule_set_id.to_string(), enabled);
    }

    pub fn toggle_rule(&mut self, rule_set_id: &str, rule_id: &str, enabled: bool) {
        if let Some(rule_set) = self.rule_sets.get_mut(rule_set_id) {
            if let Some(rule) = rule_set.rules.iter_mut().find(|r| r.id == rule_id) {
                rule.enabled = enabled;
            }
        }
    }

    pub fn get_active_rules(&self) -> Vec<&Rule> {
        self.rule_sets
            .values()
            .filter(|rs| rs.enabled)
            .flat_map(|rs| rs.rules.iter())
            .filter(|r| r.enabled)
            .collect()
    }

    pub fn get_rules_content(&self) -> String {
        let active = self.get_active_rules();
        if active.is_empty() {
            return String::new();
        }

        let mut sorted: Vec<_> = active;
        sorted.sort_by(|a, b| b.priority.cmp(&a.priority));

        let mut output = String::from("## Custom Rules\n\n");
        for rule in sorted {
            output.push_str(&rule.content);
            output.push_str("\n\n");
        }

        output
    }

    pub fn add_custom_rule(&mut self, name: &str, content: impl Into<String>) {
        let rule = Rule::new(name, content);

        if let Some(rule_set) = self.rule_sets.get_mut("custom") {
            rule_set.rules.push(rule);
        } else {
            self.rule_sets.insert(
                "custom".to_string(),
                RuleSet {
                    source: RuleSource::Custom {
                        name: "custom".to_string(),
                    },
                    rules: vec![rule],
                    enabled: true,
                },
            );
        }
    }

    pub fn list_rule_sets(&self) -> Vec<(&String, &RuleSet)> {
        self.rule_sets.iter().collect()
    }

    pub fn sync_toggles(&mut self, toggles: HashMap<String, bool>) {
        for (id, enabled) in toggles {
            self.toggle(&id, enabled);
        }
    }

    pub fn get_toggles(&self) -> &HashMap<String, bool> {
        &self.toggles
    }
}

impl Default for RulesManager {
    fn default() -> Self {
        Self::new(".")
    }
}
