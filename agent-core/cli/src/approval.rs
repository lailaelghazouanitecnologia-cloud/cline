#![deny(clippy::all)]

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalLevel {
    Auto,
    SafeRead,
    SafeCommand,
    FileWrite,
    FileDelete,
    NetworkAccess,
    SystemCommand,
    DangerousCommand,
}

impl ApprovalLevel {
    pub fn risk_score(&self) -> u8 {
        match self {
            ApprovalLevel::Auto => 0,
            ApprovalLevel::SafeRead => 1,
            ApprovalLevel::SafeCommand => 2,
            ApprovalLevel::FileWrite => 5,
            ApprovalLevel::NetworkAccess => 6,
            ApprovalLevel::FileDelete => 7,
            ApprovalLevel::SystemCommand => 8,
            ApprovalLevel::DangerousCommand => 10,
        }
    }

    pub fn requires_confirmation(&self) -> bool {
        self.risk_score() >= 5
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalRequest {
    pub id: String,
    pub tool_name: String,
    pub level: ApprovalLevel,
    pub risk_score: u8,
    pub description: String,
    pub details: serde_json::Value,
    pub auto_approve_suggestion: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalSettings {
    pub yolo_mode: bool,
    pub auto_approve_reads: bool,
    pub auto_approve_safe_commands: bool,
    pub auto_approve_writes: bool,
    pub auto_approve_network: bool,
    pub allowed_write_paths: Vec<PathBuf>,
    pub blocked_paths: Vec<PathBuf>,
    pub max_auto_risk: u8,
}

impl Default for ApprovalSettings {
    fn default() -> Self {
        Self::default_safe()
    }
}

impl ApprovalSettings {
    pub fn yolo() -> Self {
        Self {
            yolo_mode: true,
            auto_approve_reads: true,
            auto_approve_safe_commands: true,
            auto_approve_writes: true,
            auto_approve_network: true,
            allowed_write_paths: vec![],
            blocked_paths: vec![],
            max_auto_risk: 10,
        }
    }

    pub fn default_safe() -> Self {
        Self {
            yolo_mode: false,
            auto_approve_reads: true,
            auto_approve_safe_commands: true,
            auto_approve_writes: false,
            auto_approve_network: false,
            allowed_write_paths: vec![],
            blocked_paths: vec![
                PathBuf::from("/etc"),
                PathBuf::from("/usr"),
                PathBuf::from("/bin"),
                PathBuf::from("/sbin"),
                PathBuf::from("/boot"),
                PathBuf::from("/root"),
                PathBuf::from("/sys"),
                PathBuf::from("/proc"),
            ],
            max_auto_risk: 3,
        }
    }

    pub fn permissive() -> Self {
        Self {
            yolo_mode: false,
            auto_approve_reads: true,
            auto_approve_safe_commands: true,
            auto_approve_writes: true,
            auto_approve_network: false,
            allowed_write_paths: vec![],
            blocked_paths: vec![
                PathBuf::from("/etc"),
                PathBuf::from("/usr"),
                PathBuf::from("/bin"),
            ],
            max_auto_risk: 5,
        }
    }
}

pub struct ApprovalChecker {
    settings: ApprovalSettings,
    safe_commands: HashSet<String>,
    dangerous_patterns: Vec<String>,
    network_commands: Vec<String>,
    system_commands: Vec<String>,
}

impl ApprovalChecker {
    pub fn new(settings: ApprovalSettings) -> Self {
        let safe_commands: HashSet<String> = [
            "ls", "dir", "pwd", "echo", "cat", "head", "tail", "grep", "find",
            "wc", "sort", "uniq", "diff", "which", "whereis", "whoami", "date",
            "cal", "uptime", "df", "du", "free", "ps", "top", "env", "printenv",
            "tree", "file", "stat", "id", "groups", "hostname", "uname",
            "cargo", "rustc", "npm", "node", "bun", "deno", "python", "pip",
            "git status", "git log", "git diff", "git branch", "git show",
            "git rev-parse", "git remote", "git config --get",
        ].iter().map(|s| s.to_string()).collect();

        let dangerous_patterns = vec![
            "rm -rf".to_string(),
            "rm -r /".to_string(),
            "sudo".to_string(),
            "chmod 777".to_string(),
            "chown".to_string(),
            "> /".to_string(),
            "curl | sh".to_string(),
            "curl | bash".to_string(),
            "wget | sh".to_string(),
            "eval".to_string(),
            "exec".to_string(),
            "kill -9".to_string(),
            "pkill".to_string(),
            "shutdown".to_string(),
            "reboot".to_string(),
            "mkfs".to_string(),
            "dd if".to_string(),
            ":(){ :|:& };:".to_string(),
            "fork bomb".to_string(),
            "> /dev/".to_string(),
        ];

        let network_commands = vec![
            "curl".to_string(),
            "wget".to_string(),
            "fetch".to_string(),
            "nc".to_string(),
            "netcat".to_string(),
            "ssh".to_string(),
            "scp".to_string(),
            "rsync".to_string(),
            "ftp".to_string(),
            "sftp".to_string(),
        ];

        let system_commands = vec![
            "systemctl".to_string(),
            "service".to_string(),
            "mount".to_string(),
            "umount".to_string(),
            "fdisk".to_string(),
            "parted".to_string(),
            "apt".to_string(),
            "apt-get".to_string(),
            "yum".to_string(),
            "dnf".to_string(),
            "pacman".to_string(),
            "brew".to_string(),
        ];

        Self {
            settings,
            safe_commands,
            dangerous_patterns,
            network_commands,
            system_commands,
        }
    }

    pub fn check_tool(&self, tool_name: &str, input: &serde_json::Value) -> ApprovalLevel {
        if self.settings.yolo_mode {
            return ApprovalLevel::Auto;
        }

        let level = match tool_name {
            "read_file" | "list_files" | "search_files" | "list_code_definitions" => {
                if self.settings.auto_approve_reads {
                    ApprovalLevel::Auto
                } else {
                    ApprovalLevel::SafeRead
                }
            }
            "write_file" | "replace_in_file" | "insert_code_block" => {
                self.check_file_write(input)
            }
            "shell" | "execute_command" => self.check_command(input),
            "web_fetch" | "web_search" => {
                if self.settings.auto_approve_network {
                    ApprovalLevel::Auto
                } else {
                    ApprovalLevel::NetworkAccess
                }
            }
            "apply_patch" => ApprovalLevel::FileWrite,
            _ => ApprovalLevel::SafeCommand,
        };

        if level.risk_score() <= self.settings.max_auto_risk {
            ApprovalLevel::Auto
        } else {
            level
        }
    }

    fn check_file_write(&self, input: &serde_json::Value) -> ApprovalLevel {
        let path_str = input
            .get("path")
            .or_else(|| input.get("file_path"))
            .and_then(|p| p.as_str())
            .unwrap_or("");

        let path = PathBuf::from(path_str);

        if self.is_blocked_path(&path) {
            return ApprovalLevel::DangerousCommand;
        }

        if self.is_allowed_write_path(&path) {
            return ApprovalLevel::Auto;
        }

        if self.settings.auto_approve_writes {
            ApprovalLevel::Auto
        } else {
            ApprovalLevel::FileWrite
        }
    }

    fn is_blocked_path(&self, path: &PathBuf) -> bool {
        self.settings.blocked_paths.iter().any(|blocked| {
            path.starts_with(blocked)
        })
    }

    fn is_allowed_write_path(&self, path: &PathBuf) -> bool {
        if self.settings.allowed_write_paths.is_empty() {
            return false;
        }
        self.settings.allowed_write_paths.iter().any(|allowed| {
            path.starts_with(allowed)
        })
    }

    fn check_command(&self, input: &serde_json::Value) -> ApprovalLevel {
        let command = input
            .get("command")
            .and_then(|c| c.as_str())
            .unwrap_or("");

        if self.is_dangerous_command(command) {
            return ApprovalLevel::DangerousCommand;
        }

        if self.is_system_command(command) {
            return ApprovalLevel::SystemCommand;
        }

        if self.is_network_command(command) {
            if self.settings.auto_approve_network {
                return ApprovalLevel::Auto;
            }
            return ApprovalLevel::NetworkAccess;
        }

        if self.is_safe_command(command) && self.settings.auto_approve_safe_commands {
            return ApprovalLevel::Auto;
        }

        ApprovalLevel::SafeCommand
    }

    fn is_safe_command(&self, command: &str) -> bool {
        let cmd_lower = command.to_lowercase();
        let first_word = cmd_lower.split_whitespace().next().unwrap_or("");

        self.safe_commands.contains(first_word) ||
        self.safe_commands.iter().any(|safe| cmd_lower.starts_with(safe))
    }

    fn is_dangerous_command(&self, command: &str) -> bool {
        let cmd_lower = command.to_lowercase();
        self.dangerous_patterns.iter().any(|pattern| cmd_lower.contains(pattern))
    }

    fn is_network_command(&self, command: &str) -> bool {
        let first_word = command.split_whitespace().next().unwrap_or("");
        self.network_commands.iter().any(|nc| first_word == nc)
    }

    fn is_system_command(&self, command: &str) -> bool {
        let first_word = command.split_whitespace().next().unwrap_or("");
        self.system_commands.iter().any(|sc| first_word == sc)
    }

    pub fn create_request(
        &self,
        id: &str,
        tool_name: &str,
        input: &serde_json::Value,
    ) -> Option<ApprovalRequest> {
        let level = self.check_tool(tool_name, input);

        if level == ApprovalLevel::Auto {
            return None;
        }

        let description = match tool_name {
            "shell" | "execute_command" => {
                let cmd = input.get("command").and_then(|c| c.as_str()).unwrap_or("unknown");
                format!("Execute: {}", truncate(cmd, 100))
            }
            "write_file" | "replace_in_file" => {
                let path = input.get("path")
                    .or_else(|| input.get("file_path"))
                    .and_then(|p| p.as_str())
                    .unwrap_or("unknown");
                format!("Write to: {}", path)
            }
            "web_fetch" | "web_search" => {
                let url = input.get("url")
                    .or_else(|| input.get("query"))
                    .and_then(|u| u.as_str())
                    .unwrap_or("unknown");
                format!("Network: {}", truncate(url, 100))
            }
            _ => format!("Tool: {}", tool_name),
        };

        Some(ApprovalRequest {
            id: id.to_string(),
            tool_name: tool_name.to_string(),
            level,
            risk_score: level.risk_score(),
            description,
            details: input.clone(),
            auto_approve_suggestion: level.risk_score() <= 3,
        })
    }
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() > max_len {
        format!("{}...", &s[..max_len])
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_safe_commands() {
        let checker = ApprovalChecker::new(ApprovalSettings::default_safe());

        let input = serde_json::json!({"command": "ls -la"});
        assert_eq!(checker.check_tool("shell", &input), ApprovalLevel::Auto);

        let input = serde_json::json!({"command": "git status"});
        assert_eq!(checker.check_tool("shell", &input), ApprovalLevel::Auto);
    }

    #[test]
    fn test_dangerous_commands() {
        let checker = ApprovalChecker::new(ApprovalSettings::default_safe());

        let input = serde_json::json!({"command": "rm -rf /"});
        assert_eq!(checker.check_tool("shell", &input), ApprovalLevel::DangerousCommand);

        let input = serde_json::json!({"command": "sudo apt install something"});
        assert_eq!(checker.check_tool("shell", &input), ApprovalLevel::DangerousCommand);
    }

    #[test]
    fn test_network_commands() {
        let checker = ApprovalChecker::new(ApprovalSettings::default_safe());

        let input = serde_json::json!({"command": "curl https://example.com"});
        assert_eq!(checker.check_tool("shell", &input), ApprovalLevel::NetworkAccess);
    }

    #[test]
    fn test_blocked_paths() {
        let checker = ApprovalChecker::new(ApprovalSettings::default_safe());

        let input = serde_json::json!({"path": "/etc/passwd"});
        assert_eq!(checker.check_tool("write_file", &input), ApprovalLevel::DangerousCommand);
    }

    #[test]
    fn test_yolo_mode() {
        let checker = ApprovalChecker::new(ApprovalSettings::yolo());

        let input = serde_json::json!({"command": "rm -rf /"});
        assert_eq!(checker.check_tool("shell", &input), ApprovalLevel::Auto);
    }

    #[test]
    fn test_risk_scores() {
        assert!(ApprovalLevel::DangerousCommand.risk_score() > ApprovalLevel::FileWrite.risk_score());
        assert!(ApprovalLevel::FileWrite.risk_score() > ApprovalLevel::SafeCommand.risk_score());
    }
}
