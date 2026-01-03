#![deny(clippy::all)]

use agent_common::{AgentError, AgentResult};
use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;

pub struct Repl {
    editor: DefaultEditor,
}

impl Repl {
    pub fn new() -> AgentResult<Self> {
        let editor = DefaultEditor::new()
            .map_err(|e| AgentError::api(format!("Failed to create editor: {}", e)))?;

        Ok(Self { editor })
    }

    pub fn readline(&mut self) -> AgentResult<String> {
        match self.editor.readline(">>> ") {
            Ok(line) => {
                let _ = self.editor.add_history_entry(&line);
                Ok(line)
            }
            Err(ReadlineError::Interrupted) => {
                Err(AgentError::api("Interrupted"))
            }
            Err(ReadlineError::Eof) => {
                Err(AgentError::api("EOF"))
            }
            Err(e) => {
                Err(AgentError::api(format!("Readline error: {}", e)))
            }
        }
    }
}
