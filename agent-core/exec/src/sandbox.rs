#![deny(clippy::all)]
#![forbid(unsafe_code)]

use agent_common::{AgentError, AgentResult};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxConfig {
    pub allowed_commands: HashSet<String>,
    pub blocked_patterns: Vec<String>,
    pub allowed_paths: Vec<PathBuf>,
    pub blocked_paths: Vec<PathBuf>,
    pub max_output_bytes: usize,
    pub require_approval_threshold: usize,
    pub env_allowlist: HashSet<String>,
    pub env_blocklist: HashSet<String>,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        let mut allowed = HashSet::new();
        for cmd in &[
            "ls", "cat", "head", "tail", "grep", "find", "echo", "pwd", "which", "file", "wc",
            "sort", "uniq", "diff", "tr", "cut", "sed", "awk", "jq", "tree", "stat", "date",
            "node", "npm", "npx", "yarn", "pnpm", "bun", "deno",
            "python", "python3", "pip", "pip3", "poetry", "pipenv",
            "cargo", "rustc", "rustfmt", "clippy",
            "go", "gofmt",
            "git", "gh",
            "make", "cmake",
            "docker", "docker-compose",
            "kubectl", "helm",
            "curl", "wget",
            "tar", "zip", "unzip", "gzip", "gunzip",
            "mv", "cp", "mkdir", "rm", "touch", "chmod",
            "tsc", "eslint", "prettier", "jest", "vitest", "mocha",
            "rg", "fd", "bat", "exa",
        ] {
            allowed.insert((*cmd).to_string());
        }

        let blocked_patterns = vec![
            "rm -rf /".to_string(),
            "rm -rf /*".to_string(),
            ":(){ :|:& };:".to_string(),
            "> /dev/sda".to_string(),
            "mkfs.".to_string(),
            "dd if=".to_string(),
            "chmod -R 777 /".to_string(),
            "curl | sh".to_string(),
            "curl | bash".to_string(),
            "wget | sh".to_string(),
            "wget | bash".to_string(),
        ];

        let mut env_blocklist = HashSet::new();
        for key in &[
            "AWS_SECRET_ACCESS_KEY",
            "AWS_SESSION_TOKEN",
            "GITHUB_TOKEN",
            "GH_TOKEN",
            "OPENAI_API_KEY",
            "ANTHROPIC_API_KEY",
            "DATABASE_PASSWORD",
            "DB_PASSWORD",
            "SECRET_KEY",
            "PRIVATE_KEY",
        ] {
            env_blocklist.insert((*key).to_string());
        }

        Self {
            allowed_commands: allowed,
            blocked_patterns,
            allowed_paths: vec![],
            blocked_paths: vec![
                PathBuf::from("/etc/passwd"),
                PathBuf::from("/etc/shadow"),
                PathBuf::from("/etc/sudoers"),
            ],
            max_output_bytes: 1024 * 1024,
            require_approval_threshold: 100,
            env_allowlist: HashSet::new(),
            env_blocklist,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CommandValidation {
    pub allowed: bool,
    pub requires_approval: bool,
    pub blocked_reason: Option<String>,
    pub risk_level: RiskLevel,
    pub parsed_command: Option<String>,
    pub parsed_args: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    Safe,
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone)]
pub struct CommandResult {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub truncated: bool,
    pub duration_ms: u64,
}

pub struct SafeExecutor {
    config: SandboxConfig,
    working_dir: PathBuf,
    env_vars: HashMap<String, String>,
}

impl SafeExecutor {
    pub fn new(working_dir: impl Into<PathBuf>) -> Self {
        Self {
            config: SandboxConfig::default(),
            working_dir: working_dir.into(),
            env_vars: HashMap::new(),
        }
    }

    pub fn with_config(mut self, config: SandboxConfig) -> Self {
        self.config = config;
        self
    }

    pub fn allow_command(&mut self, cmd: impl Into<String>) {
        self.config.allowed_commands.insert(cmd.into());
    }

    pub fn block_command(&mut self, cmd: impl Into<String>) {
        self.config.allowed_commands.remove(&cmd.into());
    }

    pub fn allow_path(&mut self, path: impl Into<PathBuf>) {
        self.config.allowed_paths.push(path.into());
    }

    pub fn block_path(&mut self, path: impl Into<PathBuf>) {
        self.config.blocked_paths.push(path.into());
    }

    pub fn set_env(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.env_vars.insert(key.into(), value.into());
    }

    pub fn validate(&self, command: &str) -> CommandValidation {
        let (cmd, args) = self.parse_command(command);

        if let Some(reason) = self.check_blocked_patterns(command) {
            return CommandValidation {
                allowed: false,
                requires_approval: false,
                blocked_reason: Some(reason),
                risk_level: RiskLevel::Critical,
                parsed_command: cmd,
                parsed_args: args,
            };
        }

        let Some(ref parsed_cmd) = cmd else {
            return CommandValidation {
                allowed: false,
                requires_approval: false,
                blocked_reason: Some("could not parse command".to_string()),
                risk_level: RiskLevel::High,
                parsed_command: None,
                parsed_args: vec![],
            };
        };

        if !self.config.allowed_commands.contains(parsed_cmd) {
            return CommandValidation {
                allowed: false,
                requires_approval: true,
                blocked_reason: Some(format!("command '{}' not in allowlist", parsed_cmd)),
                risk_level: RiskLevel::Medium,
                parsed_command: cmd,
                parsed_args: args,
            };
        }

        if let Some(reason) = self.check_path_access(&args) {
            return CommandValidation {
                allowed: false,
                requires_approval: false,
                blocked_reason: Some(reason),
                risk_level: RiskLevel::High,
                parsed_command: cmd,
                parsed_args: args,
            };
        }

        let risk_level = self.assess_risk(parsed_cmd, &args);
        let requires_approval = risk_level == RiskLevel::Medium || risk_level == RiskLevel::High;

        CommandValidation {
            allowed: true,
            requires_approval,
            blocked_reason: None,
            risk_level,
            parsed_command: cmd,
            parsed_args: args,
        }
    }

    pub async fn execute(&self, command: &str) -> AgentResult<CommandResult> {
        let validation = self.validate(command);

        if !validation.allowed {
            return Err(AgentError::rejected_with_reason(
                validation.blocked_reason.unwrap_or_default(),
            ));
        }

        let start = std::time::Instant::now();

        let mut cmd = Command::new("sh");
        cmd.arg("-c")
            .arg(command)
            .current_dir(&self.working_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        for (key, value) in &self.env_vars {
            if !self.config.env_blocklist.contains(key) {
                cmd.env(key, value);
            }
        }

        let mut child = cmd
            .spawn()
            .map_err(|e| AgentError::io("spawn command", e))?;

        let stdout_handle = child.stdout.take();
        let stderr_handle = child.stderr.take();

        let mut stdout = String::new();
        let mut stderr = String::new();
        let mut truncated = false;

        if let Some(out) = stdout_handle {
            let mut reader = BufReader::new(out).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                if stdout.len() + line.len() > self.config.max_output_bytes {
                    truncated = true;
                    break;
                }
                stdout.push_str(&line);
                stdout.push('\n');
            }
        }

        if let Some(err) = stderr_handle {
            let mut reader = BufReader::new(err).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                if stderr.len() + line.len() > self.config.max_output_bytes {
                    truncated = true;
                    break;
                }
                stderr.push_str(&line);
                stderr.push('\n');
            }
        }

        let status = child
            .wait()
            .await
            .map_err(|e| AgentError::io("wait for command", e))?;

        let duration_ms = start.elapsed().as_millis() as u64;

        Ok(CommandResult {
            exit_code: status.code().unwrap_or(-1),
            stdout,
            stderr,
            truncated,
            duration_ms,
        })
    }

    fn parse_command(&self, command: &str) -> (Option<String>, Vec<String>) {
        let trimmed = command.trim();
        let parts: Vec<&str> = trimmed.split_whitespace().collect();

        if parts.is_empty() {
            return (None, vec![]);
        }

        let cmd = parts[0].split('/').last().unwrap_or(parts[0]).to_string();
        let args: Vec<String> = parts[1..].iter().map(|s| (*s).to_string()).collect();

        (Some(cmd), args)
    }

    fn check_blocked_patterns(&self, command: &str) -> Option<String> {
        for pattern in &self.config.blocked_patterns {
            if command.contains(pattern) {
                return Some(format!("command contains blocked pattern: {}", pattern));
            }
        }
        None
    }

    fn check_path_access(&self, args: &[String]) -> Option<String> {
        for arg in args {
            let path = Path::new(arg);
            if !path.is_absolute() {
                continue;
            }

            for blocked in &self.config.blocked_paths {
                if path.starts_with(blocked) || path == blocked {
                    return Some(format!("access to path {} is blocked", arg));
                }
            }
        }
        None
    }

    fn assess_risk(&self, cmd: &str, args: &[String]) -> RiskLevel {
        let destructive = ["rm", "mv", "chmod", "chown"];
        if destructive.contains(&cmd) {
            if args.iter().any(|a| a.contains("-rf") || a.contains("-fr")) {
                return RiskLevel::High;
            }
            return RiskLevel::Medium;
        }

        let network = ["curl", "wget"];
        if network.contains(&cmd) {
            if args.iter().any(|a| a.contains("|")) {
                return RiskLevel::High;
            }
            return RiskLevel::Low;
        }

        let write_ops = ["docker", "kubectl", "git push", "npm publish"];
        if write_ops.iter().any(|w| cmd.starts_with(w)) {
            return RiskLevel::Medium;
        }

        RiskLevel::Safe
    }
}

impl Default for SafeExecutor {
    fn default() -> Self {
        Self::new(std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
    }
}

pub struct DockerSandbox {
    image: String,
    working_dir: PathBuf,
    mount_paths: Vec<(PathBuf, PathBuf)>,
    network_mode: NetworkMode,
    allowed_hosts: Vec<String>,
    env_vars: HashMap<String, String>,
    timeout_secs: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkMode {
    None,
    Host,
    AllowList,
}

impl DockerSandbox {
    pub fn new(working_dir: impl Into<PathBuf>) -> Self {
        Self {
            image: "ubuntu:22.04".to_string(),
            working_dir: working_dir.into(),
            mount_paths: Vec::new(),
            network_mode: NetworkMode::None,
            allowed_hosts: Vec::new(),
            env_vars: HashMap::new(),
            timeout_secs: 300,
        }
    }

    pub fn with_image(mut self, image: impl Into<String>) -> Self {
        self.image = image.into();
        self
    }

    pub fn with_network(mut self, mode: NetworkMode) -> Self {
        self.network_mode = mode;
        self
    }

    pub fn allow_host(mut self, host: impl Into<String>) -> Self {
        self.allowed_hosts.push(host.into());
        self
    }

    pub fn mount(mut self, host_path: impl Into<PathBuf>, container_path: impl Into<PathBuf>) -> Self {
        self.mount_paths.push((host_path.into(), container_path.into()));
        self
    }

    pub fn env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env_vars.insert(key.into(), value.into());
        self
    }

    pub fn timeout(mut self, secs: u64) -> Self {
        self.timeout_secs = secs;
        self
    }

    fn build_docker_args(&self, command: &str) -> Vec<String> {
        let mut args = vec![
            "run".to_string(),
            "--rm".to_string(),
            "--init".to_string(),
        ];

        match self.network_mode {
            NetworkMode::None => args.push("--network=none".to_string()),
            NetworkMode::Host => args.push("--network=host".to_string()),
            NetworkMode::AllowList => {}
        }

        args.push("--memory=512m".to_string());
        args.push("--cpus=1".to_string());
        args.push("--pids-limit=256".to_string());
        args.push("--read-only".to_string());
        args.push("--tmpfs=/tmp:rw,noexec,nosuid,size=64m".to_string());

        let workdir_str = self.working_dir.to_string_lossy();
        args.push(format!("-v={}:/workspace:rw", workdir_str));
        args.push("-w=/workspace".to_string());

        for (host, container) in &self.mount_paths {
            args.push(format!("-v={}:{}:ro", host.to_string_lossy(), container.to_string_lossy()));
        }

        for (key, value) in &self.env_vars {
            args.push(format!("-e={}={}", key, value));
        }

        args.push(format!("--stop-timeout={}", self.timeout_secs));
        args.push(self.image.clone());
        args.push("/bin/sh".to_string());
        args.push("-c".to_string());
        args.push(command.to_string());

        args
    }

    pub async fn execute(&self, command: &str) -> AgentResult<CommandResult> {
        let start = std::time::Instant::now();
        let docker_args = self.build_docker_args(command);

        let mut cmd = Command::new("docker");
        cmd.args(&docker_args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let output = cmd.output().await
            .map_err(|e| AgentError::io("docker execute", e))?;

        let duration_ms = start.elapsed().as_millis() as u64;

        Ok(CommandResult {
            exit_code: output.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            truncated: false,
            duration_ms,
        })
    }

    pub async fn is_available() -> bool {
        Command::new("docker")
            .arg("info")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await
            .map(|s| s.success())
            .unwrap_or(false)
    }
}

impl Default for DockerSandbox {
    fn default() -> Self {
        Self::new(std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
    }
}

pub struct BatchExecutor {
    executor: SafeExecutor,
    results: Vec<CommandResult>,
    stop_on_error: bool,
}

impl BatchExecutor {
    pub fn new(executor: SafeExecutor) -> Self {
        Self {
            executor,
            results: Vec::new(),
            stop_on_error: true,
        }
    }

    pub fn continue_on_error(mut self) -> Self {
        self.stop_on_error = false;
        self
    }

    pub async fn run(&mut self, commands: &[&str]) -> AgentResult<()> {
        for cmd in commands {
            match self.executor.execute(cmd).await {
                Ok(result) => {
                    let failed = result.exit_code != 0;
                    self.results.push(result);
                    if failed && self.stop_on_error {
                        break;
                    }
                }
                Err(e) => {
                    if self.stop_on_error {
                        return Err(e);
                    }
                }
            }
        }
        Ok(())
    }

    pub fn results(&self) -> &[CommandResult] {
        &self.results
    }

    pub fn all_succeeded(&self) -> bool {
        self.results.iter().all(|r| r.exit_code == 0)
    }
}
