#![deny(clippy::all)]

use crate::slash_commands::{CommandRegistry, SlashCommand};
use agent_common::{AgentError, AgentResult};
use rustyline::completion::{Completer, Pair};
use rustyline::error::ReadlineError;
use rustyline::highlight::Highlighter;
use rustyline::hint::{Hint, Hinter};
use rustyline::history::DefaultHistory;
use rustyline::validate::Validator;
use rustyline::{Context, Editor, Helper};
use std::borrow::Cow;
use std::sync::Arc;

pub struct SlashCommandCompleter {
    commands: Arc<CommandRegistry>,
}

impl SlashCommandCompleter {
    pub fn new(commands: Arc<CommandRegistry>) -> Self {
        Self { commands }
    }

    fn get_completions(&self, line: &str, pos: usize) -> Vec<Pair> {
        let before_cursor = &line[..pos];

        let slash_idx = match before_cursor.rfind('/') {
            Some(idx) => idx,
            None => return vec![],
        };

        if slash_idx > 0 {
            let char_before = before_cursor.chars().nth(slash_idx - 1);
            if let Some(c) = char_before {
                if !c.is_whitespace() {
                    return vec![];
                }
            }
        }

        let query = &before_cursor[slash_idx + 1..];
        if query.contains(char::is_whitespace) {
            return vec![];
        }

        self.commands
            .matching(query)
            .iter()
            .map(|cmd| {
                let display = format!("/{}", cmd.name);
                let replacement = format!("{} ", cmd.name);
                Pair {
                    display,
                    replacement,
                }
            })
            .collect()
    }
}

impl Completer for SlashCommandCompleter {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        _ctx: &Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Pair>)> {
        let completions = self.get_completions(line, pos);

        if completions.is_empty() {
            return Ok((pos, vec![]));
        }

        let before_cursor = &line[..pos];
        let slash_idx = before_cursor.rfind('/').unwrap_or(pos);
        Ok((slash_idx + 1, completions))
    }
}

#[derive(Clone)]
pub struct CommandHint {
    display: String,
    complete_up_to: usize,
}

impl Hint for CommandHint {
    fn display(&self) -> &str {
        &self.display
    }

    fn completion(&self) -> Option<&str> {
        if self.complete_up_to > 0 {
            Some(&self.display[..self.complete_up_to])
        } else {
            None
        }
    }
}

impl CommandHint {
    fn suffix(&self, strip_chars: usize) -> CommandHint {
        CommandHint {
            display: self.display[strip_chars..].to_string(),
            complete_up_to: self.complete_up_to.saturating_sub(strip_chars),
        }
    }
}

pub struct SlashCommandHinter {
    commands: Arc<CommandRegistry>,
}

impl SlashCommandHinter {
    pub fn new(commands: Arc<CommandRegistry>) -> Self {
        Self { commands }
    }
}

impl Hinter for SlashCommandHinter {
    type Hint = CommandHint;

    fn hint(&self, line: &str, pos: usize, _ctx: &Context<'_>) -> Option<CommandHint> {
        if pos < line.len() {
            return None;
        }

        let before_cursor = &line[..pos];
        let slash_idx = before_cursor.rfind('/')?;

        if slash_idx > 0 {
            let char_before = before_cursor.chars().nth(slash_idx - 1)?;
            if !char_before.is_whitespace() {
                return None;
            }
        }

        let query = &before_cursor[slash_idx + 1..];
        if query.is_empty() || query.contains(char::is_whitespace) {
            return None;
        }

        let matches = self.commands.matching(query);
        let first_match = matches.first()?;

        if first_match.name == query {
            return None;
        }

        let suffix = &first_match.name[query.len()..];
        let hint_text = format!("{} ", suffix);

        Some(CommandHint {
            display: hint_text.clone(),
            complete_up_to: hint_text.len(),
        })
    }
}

pub struct SlashCommandHighlighter;

impl Highlighter for SlashCommandHighlighter {
    fn highlight<'l>(&self, line: &'l str, _pos: usize) -> Cow<'l, str> {
        let re = regex::Regex::new(r"(^|\s)(/[a-zA-Z0-9_.-]+)").unwrap();

        if re.is_match(line) {
            let highlighted = re.replace_all(line, |caps: &regex::Captures| {
                let ws = caps.get(1).map(|m| m.as_str()).unwrap_or("");
                let cmd = caps.get(2).map(|m| m.as_str()).unwrap_or("");
                format!("{}\x1b[1;36m{}\x1b[0m", ws, cmd)
            });
            Cow::Owned(highlighted.into_owned())
        } else {
            Cow::Borrowed(line)
        }
    }

    fn highlight_hint<'h>(&self, hint: &'h str) -> Cow<'h, str> {
        Cow::Owned(format!("\x1b[2m{}\x1b[0m", hint))
    }

    fn highlight_char(&self, _line: &str, _pos: usize, _forced: bool) -> bool {
        true
    }
}

pub struct ReplHelper {
    completer: SlashCommandCompleter,
    hinter: SlashCommandHinter,
    highlighter: SlashCommandHighlighter,
}

impl ReplHelper {
    pub fn new(commands: Arc<CommandRegistry>) -> Self {
        Self {
            completer: SlashCommandCompleter::new(Arc::clone(&commands)),
            hinter: SlashCommandHinter::new(commands),
            highlighter: SlashCommandHighlighter,
        }
    }
}

impl Helper for ReplHelper {}

impl Completer for ReplHelper {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        ctx: &Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Pair>)> {
        self.completer.complete(line, pos, ctx)
    }
}

impl Hinter for ReplHelper {
    type Hint = CommandHint;

    fn hint(&self, line: &str, pos: usize, ctx: &Context<'_>) -> Option<CommandHint> {
        self.hinter.hint(line, pos, ctx)
    }
}

impl Highlighter for ReplHelper {
    fn highlight<'l>(&self, line: &'l str, pos: usize) -> Cow<'l, str> {
        self.highlighter.highlight(line, pos)
    }

    fn highlight_hint<'h>(&self, hint: &'h str) -> Cow<'h, str> {
        self.highlighter.highlight_hint(hint)
    }

    fn highlight_char(&self, line: &str, pos: usize, forced: bool) -> bool {
        self.highlighter.highlight_char(line, pos, forced)
    }
}

impl Validator for ReplHelper {}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ReplMode {
    Normal,
    Plan,
    Yolo,
}

impl ReplMode {
    pub fn prompt(&self) -> &'static str {
        match self {
            ReplMode::Normal => ">>> ",
            ReplMode::Plan => "[plan] >>> ",
            ReplMode::Yolo => "[yolo] >>> ",
        }
    }

    pub fn color_prompt(&self) -> String {
        match self {
            ReplMode::Normal => "\x1b[1;32m>>>\x1b[0m ".to_string(),
            ReplMode::Plan => "\x1b[1;33m[plan]\x1b[0m >>> ".to_string(),
            ReplMode::Yolo => "\x1b[1;31m[yolo]\x1b[0m >>> ".to_string(),
        }
    }
}

pub struct Repl {
    editor: Editor<ReplHelper, DefaultHistory>,
    commands: Arc<CommandRegistry>,
    mode: ReplMode,
}

impl Repl {
    pub fn new() -> AgentResult<Self> {
        let commands = Arc::new(CommandRegistry::new());
        let helper = ReplHelper::new(Arc::clone(&commands));

        let config = rustyline::Config::builder()
            .completion_type(rustyline::CompletionType::List)
            .edit_mode(rustyline::EditMode::Emacs)
            .build();

        let mut editor = Editor::with_config(config)
            .map_err(|e| AgentError::api(format!("Failed to create editor: {}", e)))?;

        editor.set_helper(Some(helper));

        let history_path = dirs::data_local_dir()
            .map(|p| p.join("cline-agent").join("history.txt"));

        if let Some(path) = &history_path {
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = editor.load_history(path);
        }

        Ok(Self {
            editor,
            commands,
            mode: ReplMode::Normal,
        })
    }

    pub fn with_yolo(mut self, yolo: bool) -> Self {
        if yolo {
            self.mode = ReplMode::Yolo;
        }
        self
    }

    pub fn set_mode(&mut self, mode: ReplMode) {
        self.mode = mode;
    }

    pub fn mode(&self) -> ReplMode {
        self.mode
    }

    pub fn commands(&self) -> &CommandRegistry {
        &self.commands
    }

    pub fn readline(&mut self) -> AgentResult<String> {
        let prompt = self.mode.color_prompt();

        match self.editor.readline(&prompt) {
            Ok(line) => {
                let _ = self.editor.add_history_entry(&line);
                Ok(line)
            }
            Err(ReadlineError::Interrupted) => Err(AgentError::api("Interrupted")),
            Err(ReadlineError::Eof) => Err(AgentError::api("EOF")),
            Err(e) => Err(AgentError::api(format!("Readline error: {}", e))),
        }
    }

    pub fn save_history(&mut self) {
        let history_path = dirs::data_local_dir()
            .map(|p| p.join("cline-agent").join("history.txt"));

        if let Some(path) = history_path {
            let _ = self.editor.save_history(&path);
        }
    }

    pub fn print_help(&self) {
        println!("\n\x1b[1;36mAvailable Commands:\x1b[0m\n");

        let default_cmds: Vec<_> = self.commands
            .all_commands()
            .iter()
            .filter(|c| c.section == crate::slash_commands::CommandSection::Default)
            .collect();

        for cmd in default_cmds {
            let desc = cmd.description.as_deref().unwrap_or("");
            println!("  \x1b[1m/{:<12}\x1b[0m  {}", cmd.name, desc);
        }

        let custom_cmds: Vec<_> = self.commands
            .all_commands()
            .iter()
            .filter(|c| c.section == crate::slash_commands::CommandSection::Custom)
            .collect();

        if !custom_cmds.is_empty() {
            println!("\n\x1b[1;36mCustom Commands:\x1b[0m\n");
            for cmd in custom_cmds {
                println!("  \x1b[1m/{}\x1b[0m", cmd.name);
            }
        }

        println!();
    }
}

impl Drop for Repl {
    fn drop(&mut self) {
        self.save_history();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_completer() {
        let commands = Arc::new(CommandRegistry::new());
        let completer = SlashCommandCompleter::new(commands);

        let completions = completer.get_completions("/he", 3);
        assert!(!completions.is_empty());
        assert!(completions.iter().any(|p| p.display == "/help"));
    }

    #[test]
    fn test_mode_prompts() {
        assert_eq!(ReplMode::Normal.prompt(), ">>> ");
        assert_eq!(ReplMode::Plan.prompt(), "[plan] >>> ");
        assert_eq!(ReplMode::Yolo.prompt(), "[yolo] >>> ");
    }
}
