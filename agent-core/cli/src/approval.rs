#![deny(clippy::all)]

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ApprovalLevel {
    Auto,
    SafeCommand,
    DangerousCommand,
    FileWrite,
    FileDelete,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalRequest {
    pub id: String,
    pub tool_name: String,
    pub level: ApprovalLevel,
    pub description: String,
    pub details: serde_json::Value,
}

#[derive(Debug, Clone, Default)]
pub struct ApprovalSettings {
    pub yolo_mode: bool,
    pub auto_approve_reads: bool,
    pub auto_approve_safe_commands: bool,
    pub auto_approve_writes: bool,
}

impl ApprovalSettings {
    pub fn yolo() -> Self {
        Self {
            yolo_mode: true,
            auto_approve_reads: true,
            auto_approve_safe_commands: true,
            auto_approve_writes: true,
        }
    }

    pub fn default_safe() -> Self {
        Self {
            yolo_mode: false,
            auto_approve_reads: true,
            auto_approve_safe_commands: true,
            auto_approve_writes: false,
        }
    }
}

pub struct ApprovalChecker {
    settings: ApprovalSettings,
    safe_commands: HashSet<String>,
    dangerous_patterns: Vec<String>,
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
        ].iter().map(|s| s.to_string()).collect();

        let dangerous_patterns = vec![
            "rm -rf".to_string(),
            "sudo".to_string(),
            "chmod".to_string(),
            "chown".to_string(),
            "> /".to_string(),
            "curl | sh".to_string(),
            "wget | sh".to_string(),
            "eval".to_string(),
            "exec".to_string(),
            "kill".to_string(),
            "pkill".to_string(),
            "shutdown".to_string(),
            "reboot".to_string(),
            "mkfs".to_string(),
            "dd if".to_string(),
            ":(){ :|:& };:".to_string(),
        ];

        Self { settings, safe_commands, dangerous_patterns }
    }

    pub fn check_tool(&self, tool_name: &str, input: &serde_json::Value) -> ApprovalLevel {
        if self.settings.yolo_mode {
            return ApprovalLevel::Auto;
        }

        match tool_name {
            "read_file" | "list_files" | "search_files" => {
                if self.settings.auto_approve_reads {
                    ApprovalLevel::Auto
                } else {
                    ApprovalLevel::SafeCommand
                }
            }
            "write_file" | "replace_in_file" => {
                if self.settings.auto_approve_writes {
                    ApprovalLevel::Auto
                } else {
                    ApprovalLevel::FileWrite
                }
            }
            "execute_command" => self.check_command(input),
            _ => ApprovalLevel::DangerousCommand,
        }
    }

    fn check_command(&self, input: &serde_json::Value) -> ApprovalLevel {
        let command = input
            .get("command")
            .and_then(|c| c.as_str())
            .unwrap_or("");

        if self.is_dangerous_command(command) {
            return ApprovalLevel::DangerousCommand;
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
            "execute_command" => {
                let cmd = input.get("command").and_then(|c| c.as_str()).unwrap_or("unknown");
                format!("Execute command: {}", cmd)
            }
            "write_file" | "replace_in_file" => {
                let path = input.get("path").and_then(|p| p.as_str()).unwrap_or("unknown");
                format!("Write to file: {}", path)
            }
            _ => format!("Use tool: {}", tool_name),
        };

        Some(ApprovalRequest {
            id: id.to_string(),
            tool_name: tool_name.to_string(),
            level,
            description,
            details: input.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_safe_commands() {
        let checker = ApprovalChecker::new(ApprovalSettings::default_safe());

        let input = serde_json::json!({"command": "ls -la"});
        assert_eq!(checker.check_tool("execute_command", &input), ApprovalLevel::Auto);

        let input = serde_json::json!({"command": "git status"});
        assert_eq!(checker.check_tool("execute_command", &input), ApprovalLevel::Auto);
    }

    #[test]
    fn test_dangerous_commands() {
        let checker = ApprovalChecker::new(ApprovalSettings::default_safe());

        let input = serde_json::json!({"command": "rm -rf /"});
        assert_eq!(checker.check_tool("execute_command", &input), ApprovalLevel::DangerousCommand);

        let input = serde_json::json!({"command": "sudo apt install something"});
        assert_eq!(checker.check_tool("execute_command", &input), ApprovalLevel::DangerousCommand);
    }

    #[test]
    fn test_yolo_mode() {
        let checker = ApprovalChecker::new(ApprovalSettings::yolo());

        let input = serde_json::json!({"command": "rm -rf /"});
        assert_eq!(checker.check_tool("execute_command", &input), ApprovalLevel::Auto);
    }
}
