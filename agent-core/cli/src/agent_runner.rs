#![deny(clippy::all)]

use crate::cli::Args;
use crate::repl::Repl;
use crate::tool_bridge::ToolBridge;
use agent_client::providers::OpenAiProvider;
use agent_client::{ChatMessage, ChatRequest, ContentPart, MessageContent, ModelProvider, ToolDefinition};
use agent_common::{AgentError, AgentResult};
use agent_config::Config;
use colored::Colorize;
use std::sync::Arc;

pub struct AgentRunner {
    provider: OpenAiProvider,
    config: Config,
    args: Args,
}

impl AgentRunner {
    pub fn new(args: Args) -> AgentResult<Self> {
        let api_key = args.api_key().ok_or_else(|| {
            AgentError::configuration("API key not provided. Set GROQ_API_KEY or use --api-key")
        })?;

        let config = build_config(&args);
        let provider_config = config.provider()?;

        let provider = OpenAiProvider::new(&api_key, config.model_id()?)
            .with_base_url(&provider_config.base_url);

        Ok(Self { provider, config, args })
    }

    pub async fn run(&self) -> AgentResult<()> {
        if let Some(ref task) = self.args.task {
            self.run_task(task).await
        } else {
            self.run_interactive().await
        }
    }

    async fn run_task(&self, task: &str) -> AgentResult<()> {
        println!("Starting task: {}", task);
        println!("Provider: {} | Model: {}", self.args.provider, self.config.model_id()?);
        println!("---");

        let bridge = Arc::new(ToolBridge::new(self.config.clone()));

        let mut messages = vec![self.system_message()];
        messages.push(ChatMessage::user(task));

        let mut turn = 0;
        let max_turns = self.args.max_turns;

        loop {
            turn += 1;
            if turn > max_turns {
                println!("Max turns ({}) reached", max_turns);
                break;
            }

            println!("{}", format!("--- Turn {} ---", turn).cyan());

            let response = self.call_llm(&messages).await?;

            if let Some(text) = response.content.as_text() {
                if !text.is_empty() {
                    println!("{}", text);
                }
            }

            let tool_calls = extract_tool_calls(&response.content);

            if tool_calls.is_empty() {
                println!("\n--- Task Complete ---");
                println!("Turns: {}", turn);
                if let Some(msg) = response.content.as_text() {
                    println!("\nFinal message:\n{}", msg);
                }
                break;
            }

            messages.push(ChatMessage {
                role: agent_client::Role::Assistant,
                content: response.content.clone(),
            });

            for (id, name, input) in &tool_calls {
                println!("{} {}", "→".green(), name.yellow());

                let result = bridge.execute_tool(name, input.clone()).await;

                let (output, is_error) = match result {
                    Ok(out) => {
                        let preview = if out.len() > 100 {
                            format!("{}...", &out[..100])
                        } else {
                            out.clone()
                        };
                        println!("{} {} {}", "✓".green(), name, preview.dimmed());
                        (out, false)
                    }
                    Err(e) => {
                        let err_msg = e.to_string();
                        println!("{} {} {}", "✗".red(), name, err_msg.red());
                        (err_msg, true)
                    }
                };

                let content = if is_error {
                    format!("Error: {}", output)
                } else {
                    output
                };

                messages.push(ChatMessage::tool(id, content));
            }
        }

        Ok(())
    }

    async fn run_interactive(&self) -> AgentResult<()> {
        println!("Cline Agent - Interactive Mode");
        println!("Provider: {} | Model: {}", self.args.provider, self.config.model_id()?);
        println!("Type 'exit' or 'quit' to exit\n");

        let mut repl = Repl::new()?;
        let bridge = Arc::new(ToolBridge::new(self.config.clone()));

        loop {
            let input = match repl.readline() {
                Ok(line) => line,
                Err(_) => break,
            };

            let trimmed = input.trim();
            if trimmed.is_empty() {
                continue;
            }

            if trimmed == "exit" || trimmed == "quit" {
                break;
            }

            let mut messages = vec![self.system_message()];
            messages.push(ChatMessage::user(trimmed));

            match self.process_conversation(&bridge, &mut messages).await {
                Ok(response) => {
                    println!("\n{}\n", response);
                }
                Err(e) => {
                    eprintln!("Error: {}\n", e);
                }
            }
        }

        println!("Goodbye!");
        Ok(())
    }

    async fn process_conversation(
        &self,
        bridge: &ToolBridge,
        messages: &mut Vec<ChatMessage>,
    ) -> AgentResult<String> {
        let max_turns = self.args.max_turns;

        for turn in 1..=max_turns {
            let response = self.call_llm(messages).await?;

            let tool_calls = extract_tool_calls(&response.content);

            if tool_calls.is_empty() {
                return Ok(response.content.as_text().unwrap_or("Done").to_string());
            }

            println!("{}", format!("Turn {}", turn).dimmed());

            messages.push(ChatMessage {
                role: agent_client::Role::Assistant,
                content: response.content.clone(),
            });

            for (id, name, input) in &tool_calls {
                println!("{} {}", "→".green(), name.yellow());

                let result = bridge.execute_tool(name, input.clone()).await;

                let output = match result {
                    Ok(out) => out,
                    Err(e) => format!("Error: {}", e),
                };

                messages.push(ChatMessage::tool(id, output));
            }
        }

        Err(AgentError::api("Max turns exceeded"))
    }

    async fn call_llm(&self, messages: &[ChatMessage]) -> AgentResult<agent_client::ChatResponse> {
        let tools = get_tool_definitions();

        let request = ChatRequest {
            messages: messages.to_vec(),
            tools: Some(tools),
            max_tokens: Some(4096),
            temperature: Some(0.7),
            stop: None,
        };

        self.provider.chat(request).await
    }

    fn system_message(&self) -> ChatMessage {
        let prompt = format!(
            r#"You are an AI coding assistant. You help users with programming tasks.

Working directory: {}

Available tools:
- read_file: Read file contents
- write_file: Write content to a file
- replace_in_file: Replace text in a file
- execute_command: Run shell commands
- list_files: List directory contents
- search_files: Search for patterns in files

Always explain your actions. When you have enough information, provide your answer WITHOUT calling more tools."#,
            self.config.working_directory.display()
        );

        ChatMessage::system(prompt)
    }
}

fn extract_tool_calls(content: &MessageContent) -> Vec<(String, String, serde_json::Value)> {
    content
        .as_parts()
        .map(|parts| {
            parts
                .iter()
                .filter_map(|part| {
                    if let ContentPart::ToolUse { id, name, input } = part {
                        Some((id.clone(), name.clone(), input.clone()))
                    } else {
                        None
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}

fn get_tool_definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "read_file".to_string(),
            description: "Read the contents of a file".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "The path to the file to read"
                    }
                },
                "required": ["path"]
            }),
        },
        ToolDefinition {
            name: "write_file".to_string(),
            description: "Write content to a file".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "The path to the file to write"
                    },
                    "content": {
                        "type": "string",
                        "description": "The content to write to the file"
                    }
                },
                "required": ["path", "content"]
            }),
        },
        ToolDefinition {
            name: "execute_command".to_string(),
            description: "Execute a shell command".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "The command to execute"
                    }
                },
                "required": ["command"]
            }),
        },
        ToolDefinition {
            name: "list_files".to_string(),
            description: "List files in a directory".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "The directory path to list"
                    },
                    "recursive": {
                        "type": "boolean",
                        "description": "Whether to list recursively"
                    }
                },
                "required": ["path"]
            }),
        },
        ToolDefinition {
            name: "search_files".to_string(),
            description: "Search for a pattern in files".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "The directory to search in"
                    },
                    "pattern": {
                        "type": "string",
                        "description": "The regex pattern to search for"
                    },
                    "file_pattern": {
                        "type": "string",
                        "description": "Optional file glob pattern"
                    }
                },
                "required": ["path", "pattern"]
            }),
        },
    ]
}

fn build_config(args: &Args) -> Config {
    let mut config = Config::default();

    config.provider_id = args.provider.clone();
    config.working_directory = args.working_directory();

    if let Some(ref model) = args.model {
        config.model_id = Some(model.clone());
    }

    config
}
