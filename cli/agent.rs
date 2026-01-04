use agent_client::providers::OpenAiProvider;
use agent_client::{
    ChatMessage as AgentMessage, ChatRequest, ChatStream, ContentPart, DeltaType, MessageContent,
    ModelProvider, StreamEventType, ToolDefinition,
};
use agent_config::Config;
use agent_tools::{handlers::register_defaults, ToolCall, ToolContext, ToolRegistry, ToolSpec};
use futures::StreamExt;
use std::path::PathBuf;
use std::sync::{mpsc::Sender, Arc};

use crate::StreamMessage;

const PROVIDER_URL: &str = "https://api.groq.com/openai/v1";

pub struct AgentBridge {
    api_key: String,
    model: String,
    registry: ToolRegistry,
    config: Arc<Config>,
}

impl AgentBridge {
    pub fn new(api_key: &str, model: &str) -> Self {
        let mut registry = ToolRegistry::new();
        register_defaults(&mut registry);

        let mut config = Config::default();
        config.working_directory = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        config.timeout_ms = 60000;

        Self {
            api_key: api_key.to_string(),
            model: model.to_string(),
            registry,
            config: Arc::new(config),
        }
    }

    fn tool_specs_to_definitions(&self) -> Vec<ToolDefinition> {
        self.registry
            .specs()
            .into_iter()
            .map(|spec| {
                let schema = spec.to_json_schema();
                ToolDefinition {
                    name: spec.name,
                    description: spec.description,
                    parameters: schema,
                }
            })
            .collect()
    }

    pub fn call_streaming_sync(&self, messages: Vec<(String, String)>, tx: Sender<StreamMessage>) {
        let api_key = self.api_key.clone();
        let model = self.model.clone();
        let tools = self.tool_specs_to_definitions();

        let rt = tokio::runtime::Runtime::new().unwrap();

        rt.block_on(async move {
            let provider = OpenAiProvider::new(&api_key, &model).with_base_url(PROVIDER_URL);

            let chat_messages: Vec<AgentMessage> = messages
                .into_iter()
                .map(|(role, content)| match role.as_str() {
                    "system" => AgentMessage::system(&content),
                    "user" => AgentMessage::user(&content),
                    "assistant" => AgentMessage::assistant(&content),
                    _ => AgentMessage::user(&content),
                })
                .collect();

            let request = ChatRequest {
                messages: chat_messages,
                tools: Some(tools),
                max_tokens: Some(8192),
                temperature: Some(0.7),
                stop: None,
            };

            match provider.chat_stream(request).await {
                Ok(stream) => {
                    let mut chat_stream = ChatStream::new(stream);

                    while let Some(event_result) = chat_stream.next().await {
                        match event_result {
                            Ok(event) => {
                                if let StreamEventType::ContentBlockDelta { delta } =
                                    &event.event_type
                                {
                                    match &delta.delta_type {
                                        DeltaType::TextDelta { text } => {
                                            if tx.send(StreamMessage::Chunk(text.clone())).is_err()
                                            {
                                                break;
                                            }
                                        }
                                        DeltaType::ToolUseStart { id, name } => {
                                            let msg = format!("\n🔧 [{}] ", name);
                                            let _ = tx.send(StreamMessage::Chunk(msg));
                                        }
                                        DeltaType::ToolUseInput { input_json } => {
                                            let _ = tx.send(StreamMessage::Chunk(".".to_string()));
                                        }
                                        DeltaType::ToolUseEnd => {
                                            let _ = tx.send(StreamMessage::Chunk("✓\n".to_string()));
                                        }
                                        _ => {}
                                    }
                                }
                            }
                            Err(e) => {
                                let _ = tx.send(StreamMessage::Error(e.to_string()));
                                return;
                            }
                        }
                    }
                    let _ = tx.send(StreamMessage::Done);
                }
                Err(e) => {
                    let _ = tx.send(StreamMessage::Error(e.to_string()));
                }
            }
        });
    }

    pub fn call_with_tools_sync(&self, messages: Vec<(String, String)>, tx: Sender<StreamMessage>) {
        let api_key = self.api_key.clone();
        let model = self.model.clone();
        let tool_defs = self.tool_specs_to_definitions();
        let config = self.config.clone();
        let registry_specs = self.registry.specs();

        let rt = tokio::runtime::Runtime::new().unwrap();

        rt.block_on(async move {
            let provider = OpenAiProvider::new(&api_key, &model).with_base_url(PROVIDER_URL);
            let tool_context = ToolContext::new(config);

            let mut conversation: Vec<AgentMessage> = messages
                .into_iter()
                .map(|(role, content)| match role.as_str() {
                    "system" => AgentMessage::system(&content),
                    "user" => AgentMessage::user(&content),
                    "assistant" => AgentMessage::assistant(&content),
                    _ => AgentMessage::user(&content),
                })
                .collect();

            let mut iteration = 0;
            const MAX_ITERATIONS: usize = 10;

            loop {
                if iteration >= MAX_ITERATIONS {
                    let _ = tx.send(StreamMessage::Chunk(
                        "\n⚠️ Max iterations reached\n".to_string(),
                    ));
                    break;
                }
                iteration += 1;

                let request = ChatRequest {
                    messages: conversation.clone(),
                    tools: Some(tool_defs.clone()),
                    max_tokens: Some(8192),
                    temperature: Some(0.7),
                    stop: None,
                };

                let response = match provider.chat(request).await {
                    Ok(r) => r,
                    Err(e) => {
                        let _ = tx.send(StreamMessage::Error(e.to_string()));
                        return;
                    }
                };

                let mut has_tool_calls = false;
                let mut tool_calls_to_execute = Vec::new();

                match &response.content {
                    MessageContent::Text(text) => {
                        let _ = tx.send(StreamMessage::Chunk(text.clone()));
                    }
                    MessageContent::Parts(parts) => {
                        for part in parts {
                            match part {
                                ContentPart::Text { text } => {
                                    let _ = tx.send(StreamMessage::Chunk(text.clone()));
                                }
                                ContentPart::ToolUse { id, name, input } => {
                                    has_tool_calls = true;
                                    let _ = tx.send(StreamMessage::Chunk(format!(
                                        "\n🔧 Calling {} ",
                                        name
                                    )));
                                    tool_calls_to_execute.push((
                                        id.clone(),
                                        name.clone(),
                                        input.clone(),
                                    ));
                                }
                                _ => {}
                            }
                        }
                    }
                }

                if !has_tool_calls {
                    break;
                }

                conversation.push(AgentMessage::assistant_with_content(response.content.clone()));

                for (id, name, input) in tool_calls_to_execute {
                    let _ = tx.send(StreamMessage::Chunk("...".to_string()));

                    let result = execute_tool_simple(&name, &input);

                    let _ = tx.send(StreamMessage::Chunk(format!(" ✓\n")));

                    conversation.push(AgentMessage::tool_result(&id, &result));
                }
            }

            let _ = tx.send(StreamMessage::Done);
        });
    }
}

fn execute_tool_simple(name: &str, input: &serde_json::Value) -> String {
    match name {
        "read_file" => {
            let path = input.get("path").and_then(|p| p.as_str()).unwrap_or("");
            match std::fs::read_to_string(path) {
                Ok(content) => content,
                Err(e) => format!("Error: {}", e),
            }
        }
        "write_file" => {
            let path = input.get("path").and_then(|p| p.as_str()).unwrap_or("");
            let content = input.get("content").and_then(|c| c.as_str()).unwrap_or("");
            match std::fs::write(path, content) {
                Ok(_) => format!("File written: {}", path),
                Err(e) => format!("Error: {}", e),
            }
        }
        "list_files" => {
            let path = input.get("path").and_then(|p| p.as_str()).unwrap_or(".");
            match std::fs::read_dir(path) {
                Ok(entries) => {
                    let files: Vec<String> = entries
                        .filter_map(|e| e.ok())
                        .map(|e| e.file_name().to_string_lossy().to_string())
                        .collect();
                    files.join("\n")
                }
                Err(e) => format!("Error: {}", e),
            }
        }
        "execute_command" | "shell" => {
            let cmd = input.get("command").and_then(|c| c.as_str()).unwrap_or("");
            match std::process::Command::new("sh").arg("-c").arg(cmd).output() {
                Ok(output) => {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    if output.status.success() {
                        stdout.to_string()
                    } else {
                        format!("Exit code: {}\n{}{}", output.status, stdout, stderr)
                    }
                }
                Err(e) => format!("Error: {}", e),
            }
        }
        "search_files" => {
            let pattern = input.get("pattern").and_then(|p| p.as_str()).unwrap_or("");
            let path = input.get("path").and_then(|p| p.as_str()).unwrap_or(".");
            match std::process::Command::new("grep")
                .args(["-rn", pattern, path])
                .output()
            {
                Ok(output) => String::from_utf8_lossy(&output.stdout).to_string(),
                Err(e) => format!("Error: {}", e),
            }
        }
        _ => format!("Tool '{}' not implemented", name),
    }
}

pub fn get_model() -> String {
    std::env::var("MODEL").unwrap_or_else(|_| "openai/gpt-oss-20b".to_string())
}

pub fn get_system_prompt() -> String {
    r#"You are an AI coding assistant with access to tools for file operations and command execution.

Available tools:
- read_file: Read contents of a file
- write_file: Write content to a file
- list_files: List files in a directory
- search_files: Search for patterns in files
- execute_command/shell: Execute shell commands

When you need to read, modify, or create files, or run commands, use the appropriate tool.
Be concise and clear in your responses. Use markdown for code blocks."#
        .to_string()
}
