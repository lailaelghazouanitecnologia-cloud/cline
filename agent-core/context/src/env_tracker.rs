#![deny(clippy::all)]
#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentContext {
    pub working_directory: PathBuf,
    pub os_type: OsType,
    pub shell: String,
    pub home_dir: Option<PathBuf>,
    pub user: Option<String>,
    pub env_vars: HashMap<String, String>,
    pub detected_tools: Vec<DetectedTool>,
    pub project_type: Option<ProjectType>,
    pub git_info: Option<GitInfo>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OsType {
    Linux,
    MacOS,
    Windows,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectedTool {
    pub name: String,
    pub version: Option<String>,
    pub path: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectType {
    Rust,
    Node,
    Python,
    Go,
    Java,
    CSharp,
    Ruby,
    Php,
    Swift,
    Kotlin,
    Mixed,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitInfo {
    pub is_repo: bool,
    pub branch: Option<String>,
    pub remote_url: Option<String>,
    pub has_uncommitted_changes: bool,
    pub ahead_behind: Option<(usize, usize)>,
}

pub struct EnvironmentTracker {
    context: EnvironmentContext,
    important_vars: Vec<String>,
}

impl EnvironmentTracker {
    pub fn new(working_directory: impl Into<PathBuf>) -> Self {
        let working_directory = working_directory.into();

        let os_type = if cfg!(target_os = "linux") {
            OsType::Linux
        } else if cfg!(target_os = "macos") {
            OsType::MacOS
        } else if cfg!(target_os = "windows") {
            OsType::Windows
        } else {
            OsType::Unknown
        };

        let shell = std::env::var("SHELL")
            .unwrap_or_else(|_| {
                if cfg!(target_os = "windows") {
                    "cmd.exe".to_string()
                } else {
                    "/bin/sh".to_string()
                }
            });

        let home_dir = dirs::home_dir();
        let user = std::env::var("USER")
            .or_else(|_| std::env::var("USERNAME"))
            .ok();

        Self {
            context: EnvironmentContext {
                working_directory,
                os_type,
                shell,
                home_dir,
                user,
                env_vars: HashMap::new(),
                detected_tools: Vec::new(),
                project_type: None,
                git_info: None,
            },
            important_vars: vec![
                "PATH".to_string(),
                "HOME".to_string(),
                "LANG".to_string(),
                "TERM".to_string(),
                "EDITOR".to_string(),
            ],
        }
    }

    pub fn with_important_vars(mut self, vars: Vec<String>) -> Self {
        self.important_vars = vars;
        self
    }

    pub fn capture_env_vars(&mut self) {
        for var in &self.important_vars {
            if let Ok(value) = std::env::var(var) {
                self.context.env_vars.insert(var.clone(), value);
            }
        }
    }

    pub async fn detect_project_type(&mut self) {
        let wd = &self.context.working_directory;

        let project_type = if wd.join("Cargo.toml").exists() {
            ProjectType::Rust
        } else if wd.join("package.json").exists() {
            ProjectType::Node
        } else if wd.join("pyproject.toml").exists() || wd.join("setup.py").exists() {
            ProjectType::Python
        } else if wd.join("go.mod").exists() {
            ProjectType::Go
        } else if wd.join("pom.xml").exists() || wd.join("build.gradle").exists() {
            ProjectType::Java
        } else if wd.join("Gemfile").exists() {
            ProjectType::Ruby
        } else if wd.join("composer.json").exists() {
            ProjectType::Php
        } else if wd.join("Package.swift").exists() {
            ProjectType::Swift
        } else {
            ProjectType::Unknown
        };

        self.context.project_type = Some(project_type);
    }

    pub async fn detect_tools(&mut self) {
        let tools_to_check = [
            ("git", &["--version"]),
            ("node", &["--version"]),
            ("npm", &["--version"]),
            ("cargo", &["--version"]),
            ("rustc", &["--version"]),
            ("python", &["--version"]),
            ("python3", &["--version"]),
            ("go", &["version"]),
            ("java", &["-version"]),
        ];

        for (name, args) in tools_to_check {
            if let Ok(output) = tokio::process::Command::new(name)
                .args(args)
                .output()
                .await
            {
                if output.status.success() {
                    let version = String::from_utf8_lossy(&output.stdout)
                        .lines()
                        .next()
                        .map(|s| s.to_string());

                    self.context.detected_tools.push(DetectedTool {
                        name: name.to_string(),
                        version,
                        path: None,
                    });
                }
            }
        }
    }

    pub async fn detect_git_info(&mut self) {
        let wd = &self.context.working_directory;

        let is_repo = wd.join(".git").exists();
        if !is_repo {
            self.context.git_info = Some(GitInfo {
                is_repo: false,
                branch: None,
                remote_url: None,
                has_uncommitted_changes: false,
                ahead_behind: None,
            });
            return;
        }

        let branch = self.run_git(&["branch", "--show-current"]).await;
        let remote = self.run_git(&["remote", "get-url", "origin"]).await;
        let status = self.run_git(&["status", "--porcelain"]).await;

        self.context.git_info = Some(GitInfo {
            is_repo: true,
            branch,
            remote_url: remote,
            has_uncommitted_changes: status.map(|s| !s.is_empty()).unwrap_or(false),
            ahead_behind: None,
        });
    }

    async fn run_git(&self, args: &[&str]) -> Option<String> {
        let output = tokio::process::Command::new("git")
            .args(args)
            .current_dir(&self.context.working_directory)
            .output()
            .await
            .ok()?;

        if output.status.success() {
            Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
        } else {
            None
        }
    }

    pub fn get_context(&self) -> &EnvironmentContext {
        &self.context
    }

    pub fn working_directory(&self) -> &PathBuf {
        &self.context.working_directory
    }

    pub fn set_working_directory(&mut self, path: impl Into<PathBuf>) {
        self.context.working_directory = path.into();
    }

    pub fn project_type(&self) -> Option<&ProjectType> {
        self.context.project_type.as_ref()
    }

    pub fn has_tool(&self, name: &str) -> bool {
        self.context
            .detected_tools
            .iter()
            .any(|t| t.name == name)
    }

    pub fn is_git_repo(&self) -> bool {
        self.context
            .git_info
            .as_ref()
            .map(|g| g.is_repo)
            .unwrap_or(false)
    }

    pub fn to_context_string(&self) -> String {
        let mut parts = Vec::new();

        parts.push(format!(
            "Working directory: {}",
            self.context.working_directory.display()
        ));
        parts.push(format!("OS: {:?}", self.context.os_type));
        parts.push(format!("Shell: {}", self.context.shell));

        if let Some(ref pt) = self.context.project_type {
            parts.push(format!("Project type: {:?}", pt));
        }

        if let Some(ref git) = self.context.git_info {
            if git.is_repo {
                if let Some(ref branch) = git.branch {
                    parts.push(format!("Git branch: {}", branch));
                }
                if git.has_uncommitted_changes {
                    parts.push("Git: has uncommitted changes".to_string());
                }
            }
        }

        if !self.context.detected_tools.is_empty() {
            let tools: Vec<_> = self
                .context
                .detected_tools
                .iter()
                .map(|t| t.name.as_str())
                .collect();
            parts.push(format!("Available tools: {}", tools.join(", ")));
        }

        parts.join("\n")
    }
}

impl Default for EnvironmentTracker {
    fn default() -> Self {
        Self::new(std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
    }
}
