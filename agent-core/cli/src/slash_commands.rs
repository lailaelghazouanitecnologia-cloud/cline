#![deny(clippy::all)]

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum CommandSection {
    Default,
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlashCommand {
    pub name: String,
    pub description: Option<String>,
    pub section: CommandSection,
    pub cli_compatible: bool,
}

impl SlashCommand {
    pub fn new(name: &str, description: &str) -> Self {
        Self {
            name: name.to_string(),
            description: Some(description.to_string()),
            section: CommandSection::Default,
            cli_compatible: true,
        }
    }

    pub fn custom(name: &str) -> Self {
        Self {
            name: name.to_string(),
            description: None,
            section: CommandSection::Custom,
            cli_compatible: true,
        }
    }
}

pub fn default_commands() -> Vec<SlashCommand> {
    vec![
        SlashCommand::new("help", "Show available commands"),
        SlashCommand::new("newtask", "Create a new task with context from the current task"),
        SlashCommand::new("compact", "Condenses your current context window"),
        SlashCommand::new("clear", "Clear the current conversation"),
        SlashCommand::new("history", "Show session history"),
        SlashCommand::new("export", "Export the current conversation"),
        SlashCommand::new("yolo", "Toggle auto-approve mode"),
        SlashCommand::new("model", "Change the current model"),
        SlashCommand::new("provider", "Change the current provider"),
        SlashCommand::new("usage", "Show token usage and cost"),
        SlashCommand::new("plan", "Create a comprehensive implementation plan"),
        SlashCommand::new("debug", "Toggle debug mode"),
    ]
}

pub struct CommandRegistry {
    commands: Vec<SlashCommand>,
    custom_commands: HashMap<String, String>,
}

impl CommandRegistry {
    pub fn new() -> Self {
        Self {
            commands: default_commands(),
            custom_commands: HashMap::new(),
        }
    }

    pub fn add_custom(&mut self, name: &str, content: &str) {
        self.custom_commands.insert(name.to_string(), content.to_string());
        self.commands.push(SlashCommand::custom(name));
    }

    pub fn all_commands(&self) -> &[SlashCommand] {
        &self.commands
    }

    pub fn find(&self, name: &str) -> Option<&SlashCommand> {
        self.commands.iter().find(|c| c.name == name)
    }

    pub fn matching(&self, query: &str) -> Vec<&SlashCommand> {
        if query.is_empty() {
            return self.commands.iter().collect();
        }
        self.commands
            .iter()
            .filter(|c| c.name.starts_with(query))
            .collect()
    }

    pub fn get_custom_content(&self, name: &str) -> Option<&String> {
        self.custom_commands.get(name)
    }
}

impl Default for CommandRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct ParsedCommand {
    pub name: String,
    pub text_without_command: String,
    pub instruction: String,
}

pub struct SlashCommandParser {
    command_regex: Regex,
}

impl SlashCommandParser {
    pub fn new() -> Self {
        Self {
            command_regex: Regex::new(r"(^|\s)/([a-zA-Z0-9_.-]+)(\s|$)").unwrap(),
        }
    }

    pub fn parse(&self, text: &str, registry: &CommandRegistry) -> Option<ParsedCommand> {
        let caps = self.command_regex.captures(text)?;
        let command_name = caps.get(2)?.as_str();

        if registry.find(command_name).is_none() {
            return None;
        }

        let full_match = caps.get(0)?;
        let start = full_match.start();

        let whitespace_before = caps.get(1).map(|m| m.as_str()).unwrap_or("");
        let whitespace_after = caps.get(3).map(|m| m.as_str()).unwrap_or("");

        let command_start = start + whitespace_before.len();
        let command_end = full_match.end() - whitespace_after.len();

        let mut text_without = String::new();
        text_without.push_str(&text[..command_start]);
        text_without.push_str(&text[command_end..]);
        let text_without = text_without.trim().to_string();

        let instruction = self.get_instruction(command_name, registry);

        Some(ParsedCommand {
            name: command_name.to_string(),
            text_without_command: text_without,
            instruction,
        })
    }

    fn get_instruction(&self, command: &str, registry: &CommandRegistry) -> String {
        if let Some(content) = registry.get_custom_content(command) {
            return format!(
                "<explicit_instructions type=\"{}\">\n{}\n</explicit_instructions>\n",
                command, content
            );
        }

        match command {
            "help" => help_response(),
            "newtask" => new_task_response(),
            "compact" => condense_response(),
            "clear" => clear_response(),
            "history" => history_response(),
            "export" => export_response(),
            "yolo" => yolo_response(),
            "model" => model_response(),
            "provider" => provider_response(),
            "usage" => usage_response(),
            "plan" => plan_response(),
            "debug" => debug_response(),
            _ => String::new(),
        }
    }

    pub fn should_show_menu(&self, text: &str, cursor_pos: usize) -> bool {
        let before_cursor = &text[..cursor_pos.min(text.len())];

        let slash_idx = before_cursor.rfind('/');
        let slash_idx = match slash_idx {
            Some(idx) => idx,
            None => return false,
        };

        if slash_idx > 0 {
            let char_before = before_cursor.chars().nth(slash_idx - 1);
            if let Some(c) = char_before {
                if !c.is_whitespace() {
                    return false;
                }
            }
        }

        let after_slash = &before_cursor[slash_idx + 1..];
        if after_slash.contains(char::is_whitespace) {
            return false;
        }

        let before_slash = &text[..slash_idx];
        let earlier_command = Regex::new(r"(^|\s)/[a-zA-Z0-9_.-]+\s").unwrap();
        if earlier_command.is_match(before_slash) {
            return false;
        }

        true
    }

    pub fn get_query(&self, text: &str, cursor_pos: usize) -> String {
        let before_cursor = &text[..cursor_pos.min(text.len())];
        if let Some(slash_idx) = before_cursor.rfind('/') {
            before_cursor[slash_idx + 1..].to_string()
        } else {
            String::new()
        }
    }

    pub fn insert_command(&self, text: &str, command: &str, cursor_pos: usize) -> (String, usize) {
        let before_cursor = &text[..cursor_pos.min(text.len())];
        let slash_idx = before_cursor.rfind('/').unwrap_or(0);

        let before_slash = &text[..slash_idx + 1];
        let after_cursor = &text[cursor_pos..];

        let suffix = if after_cursor.starts_with(' ') { "" } else { " " };
        let new_text = format!("{}{}{}{}", before_slash, command, suffix, after_cursor.trim_start());
        let new_cursor = slash_idx + 1 + command.len() + suffix.len();

        (new_text, new_cursor)
    }

    pub fn validate(&self, command: &str, registry: &CommandRegistry) -> CommandValidation {
        if command.is_empty() {
            return CommandValidation::None;
        }

        if registry.find(command).is_some() {
            return CommandValidation::Full;
        }

        if registry.matching(command).is_empty() {
            CommandValidation::None
        } else {
            CommandValidation::Partial
        }
    }
}

impl Default for SlashCommandParser {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum CommandValidation {
    Full,
    Partial,
    None,
}

fn help_response() -> String {
    r#"<explicit_instructions type="help">
Show the user a list of available slash commands with their descriptions.
Format the output as a clean, readable list.
</explicit_instructions>
"#.to_string()
}

fn new_task_response() -> String {
    r#"<explicit_instructions type="new_task">
The user has asked to create a new task with preloaded context.
Create a detailed summary including:
1. Current Work: What was being worked on
2. Key Technical Concepts: Technologies, patterns, frameworks discussed
3. Relevant Files and Code: Files examined, modified, or created
4. Problem Solving: Problems solved and ongoing troubleshooting
5. Pending Tasks and Next Steps: Outstanding work with code snippets

Use the new_task tool to create this context.
</explicit_instructions>
"#.to_string()
}

fn condense_response() -> String {
    r#"<explicit_instructions type="condense">
The user has asked to compact the current context window.
Create a detailed summary preserving:
1. Previous Conversation: High-level discussion overview
2. Current Work: Recent work in detail
3. Key Technical Concepts: Important technical details
4. Relevant Files and Code: Files and code sections
5. Problem Solving: Solutions and troubleshooting
6. Pending Tasks and Next Steps: Outstanding work

Use the condense tool to compact the context.
</explicit_instructions>
"#.to_string()
}

fn clear_response() -> String {
    r#"<explicit_instructions type="clear">
The user wants to clear the current conversation.
Acknowledge this and start fresh.
</explicit_instructions>
"#.to_string()
}

fn history_response() -> String {
    r#"<explicit_instructions type="history">
Show the user their recent session history.
List sessions with their IDs, titles, and timestamps.
</explicit_instructions>
"#.to_string()
}

fn export_response() -> String {
    r#"<explicit_instructions type="export">
Export the current conversation to a file.
Include all messages, tool calls, and their results.
Format as markdown for readability.
</explicit_instructions>
"#.to_string()
}

fn yolo_response() -> String {
    r#"<explicit_instructions type="yolo">
Toggle YOLO (auto-approve) mode.
When enabled, all tool calls are automatically approved.
When disabled, dangerous operations require confirmation.
</explicit_instructions>
"#.to_string()
}

fn model_response() -> String {
    r#"<explicit_instructions type="model">
The user wants to change the current model.
Show available models and let the user select one.
</explicit_instructions>
"#.to_string()
}

fn provider_response() -> String {
    r#"<explicit_instructions type="provider">
The user wants to change the current provider.
Show available providers: openai, anthropic, groq.
</explicit_instructions>
"#.to_string()
}

fn usage_response() -> String {
    r#"<explicit_instructions type="usage">
Show the user their token usage and cost for the current session.
Include input tokens, output tokens, total tokens, and estimated cost.
</explicit_instructions>
"#.to_string()
}

fn plan_response() -> String {
    r#"<explicit_instructions type="plan">
The user wants to create a comprehensive implementation plan before coding.
Create a detailed plan including:
1. Requirements Analysis: What needs to be built
2. Architecture Design: High-level structure
3. Implementation Steps: Ordered list of tasks
4. Technical Decisions: Key choices and trade-offs
5. Testing Strategy: How to verify the implementation
6. Risk Assessment: Potential issues and mitigations

Present this plan for user approval before proceeding.
</explicit_instructions>
"#.to_string()
}

fn debug_response() -> String {
    r#"<explicit_instructions type="debug">
Toggle debug mode to show additional diagnostic information.
</explicit_instructions>
"#.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_command_at_start() {
        let registry = CommandRegistry::new();
        let parser = SlashCommandParser::new();

        let result = parser.parse("/help test message", &registry);
        assert!(result.is_some());
        let parsed = result.unwrap();
        assert_eq!(parsed.name, "help");
        assert_eq!(parsed.text_without_command, "test message");
    }

    #[test]
    fn test_parse_command_in_middle() {
        let registry = CommandRegistry::new();
        let parser = SlashCommandParser::new();

        let result = parser.parse("please /compact the context", &registry);
        assert!(result.is_some());
        let parsed = result.unwrap();
        assert_eq!(parsed.name, "compact");
    }

    #[test]
    fn test_no_match_in_url() {
        let registry = CommandRegistry::new();
        let parser = SlashCommandParser::new();

        let result = parser.parse("visit http://example.com/help", &registry);
        assert!(result.is_none());
    }

    #[test]
    fn test_matching_commands() {
        let registry = CommandRegistry::new();
        let matches = registry.matching("co");
        assert!(matches.iter().any(|c| c.name == "compact"));
    }

    #[test]
    fn test_should_show_menu() {
        let parser = SlashCommandParser::new();

        assert!(parser.should_show_menu("/he", 3));
        assert!(parser.should_show_menu("test /co", 8));
        assert!(!parser.should_show_menu("http://example.com/test", 23));
        assert!(!parser.should_show_menu("/help already typed", 15));
    }

    #[test]
    fn test_validation() {
        let registry = CommandRegistry::new();
        let parser = SlashCommandParser::new();

        assert_eq!(parser.validate("help", &registry), CommandValidation::Full);
        assert_eq!(parser.validate("hel", &registry), CommandValidation::Partial);
        assert_eq!(parser.validate("xyz", &registry), CommandValidation::None);
    }
}
