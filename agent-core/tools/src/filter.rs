#![deny(clippy::all)]
#![forbid(unsafe_code)]

use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolMode {
    Plan,
    Act,
    All,
}

pub struct ToolFilter {
    mode: ToolMode,
    plan_allowed: HashSet<String>,
    dangerous_tools: HashSet<String>,
    blocked_tools: HashSet<String>,
}

impl ToolFilter {
    pub fn new(mode: ToolMode) -> Self {
        let mut plan_allowed = HashSet::new();
        plan_allowed.insert("read_file".to_string());
        plan_allowed.insert("list_files".to_string());
        plan_allowed.insert("search_files".to_string());
        plan_allowed.insert("list_code_definitions".to_string());
        plan_allowed.insert("fetch_url".to_string());
        plan_allowed.insert("plan_mode_respond".to_string());

        let mut dangerous = HashSet::new();
        dangerous.insert("execute_command".to_string());
        dangerous.insert("shell".to_string());
        dangerous.insert("write_file".to_string());
        dangerous.insert("apply_patch".to_string());
        dangerous.insert("replace_in_file".to_string());
        dangerous.insert("insert_code_block".to_string());
        dangerous.insert("browser_action".to_string());

        Self {
            mode,
            plan_allowed,
            dangerous_tools: dangerous,
            blocked_tools: HashSet::new(),
        }
    }

    pub fn plan_mode() -> Self {
        Self::new(ToolMode::Plan)
    }

    pub fn act_mode() -> Self {
        Self::new(ToolMode::Act)
    }

    pub fn all_tools() -> Self {
        Self::new(ToolMode::All)
    }

    pub fn set_mode(&mut self, mode: ToolMode) {
        self.mode = mode;
    }

    pub fn block_tool(&mut self, name: impl Into<String>) {
        self.blocked_tools.insert(name.into());
    }

    pub fn unblock_tool(&mut self, name: &str) {
        self.blocked_tools.remove(name);
    }

    pub fn add_plan_allowed(&mut self, name: impl Into<String>) {
        self.plan_allowed.insert(name.into());
    }

    pub fn add_dangerous(&mut self, name: impl Into<String>) {
        self.dangerous_tools.insert(name.into());
    }

    pub fn is_allowed(&self, tool_name: &str) -> bool {
        if self.blocked_tools.contains(tool_name) {
            return false;
        }

        match self.mode {
            ToolMode::All => true,
            ToolMode::Act => true,
            ToolMode::Plan => self.plan_allowed.contains(tool_name),
        }
    }

    pub fn is_dangerous(&self, tool_name: &str) -> bool {
        self.dangerous_tools.contains(tool_name)
    }

    pub fn requires_approval(&self, tool_name: &str, auto_approve: bool) -> bool {
        if auto_approve {
            return false;
        }
        self.is_dangerous(tool_name)
    }

    pub fn filter_tools<'a>(&self, tools: &'a [String]) -> Vec<&'a String> {
        tools.iter().filter(|t| self.is_allowed(t)).collect()
    }

    pub fn get_allowed_tools(&self) -> Vec<&String> {
        match self.mode {
            ToolMode::Plan => self.plan_allowed.iter().collect(),
            ToolMode::Act | ToolMode::All => Vec::new(),
        }
    }

    pub fn get_blocked_reason(&self, tool_name: &str) -> Option<String> {
        if self.blocked_tools.contains(tool_name) {
            return Some(format!("Tool '{}' is blocked by configuration", tool_name));
        }

        if self.mode == ToolMode::Plan && !self.plan_allowed.contains(tool_name) {
            return Some(format!(
                "Tool '{}' is not available in Plan mode. Switch to Act mode to use it.",
                tool_name
            ));
        }

        None
    }
}

impl Default for ToolFilter {
    fn default() -> Self {
        Self::act_mode()
    }
}

#[derive(Debug, Clone)]
pub struct ToolPermission {
    pub tool_name: String,
    pub allowed: bool,
    pub requires_approval: bool,
    pub reason: Option<String>,
}

impl ToolFilter {
    pub fn check_permission(&self, tool_name: &str, auto_approve: bool) -> ToolPermission {
        let allowed = self.is_allowed(tool_name);
        let requires_approval = allowed && self.requires_approval(tool_name, auto_approve);
        let reason = if !allowed {
            self.get_blocked_reason(tool_name)
        } else {
            None
        };

        ToolPermission {
            tool_name: tool_name.to_string(),
            allowed,
            requires_approval,
            reason,
        }
    }
}
