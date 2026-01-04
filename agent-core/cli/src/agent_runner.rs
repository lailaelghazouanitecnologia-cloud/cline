#![deny(clippy::all)]

use crate::cli::Args;
use crate::repl::Repl;
use crate::tool_bridge::ToolBridge;
use agent_client::providers::OpenAiProvider;
use agent_client::{
    ChatMessage, ChatRequest, ChatStream, ContentPart, MessageContent, ModelProvider,
    StreamBuffer, StreamEvent, StreamEventType,
};
use agent_common::{AgentError, AgentResult};
use agent_config::Config;
use agent_tools::{ApprovalManager, ApprovalPolicy, ApprovalReason, ApprovalRequest, ApprovalContext, RiskLevel, DiffParser};
use colored::Colorize;
use futures::StreamExt;
use std::io::{self, Write};
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionMode {
    Suggest,
    AutoEdit,
    FullAuto,
}

pub struct AgentRunner {
    provider: OpenAiProvider,
    bridge: Arc<ToolBridge>,
    config: Config,
    args: Args,
    approval_manager: Arc<ApprovalManager>,
    approval_policy: Arc<RwLock<ApprovalPolicy>>,
    execution_mode: ExecutionMode,
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

        let bridge = Arc::new(ToolBridge::new(config.clone()));

        let (approval_manager, _rx) = ApprovalManager::new();
        let approval_manager = Arc::new(approval_manager);

        let execution_mode = if args.yolo {
            ExecutionMode::FullAuto
        } else if args.auto_edit {
            ExecutionMode::AutoEdit
        } else {
            ExecutionMode::Suggest
        };

        let approval_policy = Arc::new(RwLock::new(ApprovalPolicy::new()));

        Ok(Self {
            provider,
            bridge,
            config,
            args,
            approval_manager,
            approval_policy,
            execution_mode,
        })
    }

    pub async fn run(&self) -> AgentResult<()> {
        if let Some(ref task) = self.args.task {
            self.run_task(task).await
        } else {
            self.run_interactive().await
        }
    }

    async fn run_task(&self, task: &str) -> AgentResult<()> {
        self.print_header(task)?;

        let mut messages = vec![self.system_message()];
        messages.push(ChatMessage::user(task));

        let mut turn = 0;
        let max_turns = self.args.max_turns;

        loop {
            turn += 1;
            if turn > max_turns {
                println!("{}", format!("Max turns ({}) reached", max_turns).yellow());
                break;
            }

            println!("{}", format!("─── Turn {} ───", turn).cyan());

            let (content, buffer) = self.call_llm_stream(&messages).await?;
            println!();

            let tool_calls = extract_tool_calls(&content);

            if tool_calls.is_empty() {
                self.print_completion(turn, &content);
                break;
            }

            messages.push(ChatMessage {
                role: agent_client::Role::Assistant,
                content: content.clone(),
            });

            for (id, name, input) in &tool_calls {
                let approved = self.check_approval(&name, &input).await?;
                if !approved {
                    println!("{} {} {}", "⊘".red(), name.yellow(), "(denied)".dimmed());
                    messages.push(ChatMessage::tool(id, "Tool execution denied by user"));
                    continue;
                }

                self.show_tool_preview(&name, &input).await;
                let result = self.execute_tool_with_feedback(&name, input.clone()).await;
                let output = match result {
                    Ok(out) => {
                        messages.push(ChatMessage::tool(id, out.clone()));
                        out
                    }
                    Err(e) => {
                        let err = format!("Error: {}", e);
                        messages.push(ChatMessage::tool(id, err.clone()));
                        err
                    }
                };
                let _ = output;
            }
        }

        Ok(())
    }

    fn print_header(&self, task: &str) -> AgentResult<()> {
        let mode_str = match self.execution_mode {
            ExecutionMode::Suggest => "suggest",
            ExecutionMode::AutoEdit => "auto-edit",
            ExecutionMode::FullAuto => "full-auto",
        };
        println!("{}", "═".repeat(60).dimmed());
        println!("{} {}", "Task:".bold(), task);
        println!("{} {} | {} {} | {} {}",
            "Provider:".dimmed(), self.args.provider.cyan(),
            "Model:".dimmed(), self.config.model_id()?.cyan(),
            "Mode:".dimmed(), mode_str.yellow()
        );
        println!("{}", "═".repeat(60).dimmed());
        Ok(())
    }

    fn print_completion(&self, turns: u32, content: &MessageContent) {
        println!("\n{}", "═".repeat(60).green());
        println!("{} in {} turns", "✓ Task Complete".green().bold(), turns);
        if let Some(msg) = content.as_text() {
            if !msg.is_empty() {
                println!("\n{}", msg);
            }
        }
        println!("{}", "═".repeat(60).green());
    }

    async fn check_approval(&self, tool_name: &str, input: &serde_json::Value) -> AgentResult<bool> {
        if self.execution_mode == ExecutionMode::FullAuto {
            return Ok(true);
        }

        let policy = self.approval_policy.read().await;
        if !policy.requires_approval(tool_name, input) {
            return Ok(true);
        }
        drop(policy);

        if self.execution_mode == ExecutionMode::AutoEdit && is_file_edit_tool(tool_name) {
            return Ok(true);
        }

        self.prompt_user_approval(tool_name, input).await
    }

    async fn prompt_user_approval(&self, tool_name: &str, input: &serde_json::Value) -> AgentResult<bool> {
        println!("\n{} {} requires approval:", "⚠".yellow(), tool_name.yellow().bold());
        println!("{}", format_tool_input(input).dimmed());
        print!("{}", "Allow? [y/n/a(lways)]: ".cyan());
        io::stdout().flush().ok();

        let mut response = String::new();
        io::stdin().read_line(&mut response).ok();

        match response.trim().to_lowercase().as_str() {
            "y" | "yes" => Ok(true),
            "a" | "always" => {
                self.approval_manager.approve_tool(tool_name).await;
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    async fn show_tool_preview(&self, tool_name: &str, input: &serde_json::Value) {
        if tool_name == "apply_patch" {
            if let Some(patch) = input.get("patch").and_then(|v| v.as_str()) {
                let diff_view = DiffParser::parse_unified(patch);
                let formatted = DiffParser::format_for_display(&diff_view);
                println!("\n{}", "Preview:".cyan().bold());
                for line in formatted.lines() {
                    if line.starts_with("  +") {
                        println!("{}", line.green());
                    } else if line.starts_with("  -") {
                        println!("{}", line.red());
                    } else {
                        println!("{}", line);
                    }
                }
            }
        }
    }

    async fn execute_tool_with_feedback(&self, name: &str, input: serde_json::Value) -> AgentResult<String> {
        print!("{} {}", "→".green(), name.yellow());
        io::stdout().flush().ok();

        let result = self.bridge.execute_tool(name, input).await;

        match &result {
            Ok(out) => {
                let preview = truncate_output(out, 80);
                println!(" {} {}", "✓".green(), preview.dimmed());
            }
            Err(e) => {
                println!(" {} {}", "✗".red(), e.to_string().red());
            }
        }

        result
    }

    async fn run_interactive(&self) -> AgentResult<()> {
        println!("Cline Agent - Interactive Mode");
        println!("Provider: {} | Model: {}", self.args.provider, self.config.model_id()?);
        println!("Type 'exit' or 'quit' to exit\n");

        let mut repl = Repl::new()?;

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

            match self.process_conversation(&mut messages).await {
                Ok(response) => println!("\n{}\n", response),
                Err(e) => eprintln!("Error: {}\n", e),
            }
        }

        println!("Goodbye!");
        Ok(())
    }

    async fn process_conversation(&self, messages: &mut Vec<ChatMessage>) -> AgentResult<String> {
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

                let result = self.bridge.execute_tool(name, input.clone()).await;
                let output = match result {
                    Ok(out) => out,
                    Err(e) => format!("Error: {}", e),
                };

                messages.push(ChatMessage::tool(id, output));
            }
        }

        Err(AgentError::api("Max turns exceeded"))
    }

    async fn call_llm_stream(&self, messages: &[ChatMessage]) -> AgentResult<(MessageContent, StreamBuffer)> {
        let tools = self.bridge.tool_definitions();

        let request = ChatRequest {
            messages: messages.to_vec(),
            tools: Some(tools),
            max_tokens: Some(4096),
            temperature: Some(0.7),
            stop: None,
        };

        let stream = self.provider.chat_stream(request).await?;
        let mut chat_stream = ChatStream::new(stream);

        while let Some(event_result) = chat_stream.next().await {
            match event_result {
                Ok(event) => {
                    if let StreamEventType::ContentBlockDelta { delta } = &event.event_type {
                        if let agent_client::DeltaType::TextDelta { text } = &delta.delta_type {
                            print!("{}", text);
                            io::stdout().flush().ok();
                        }
                    }
                }
                Err(e) => {
                    eprintln!("\n{}: {}", "Stream error".red(), e);
                }
            }
        }

        let (_, buffer) = chat_stream.into_parts();
        let content = buffer.into_content();

        Ok((content, StreamBuffer::default()))
    }

    async fn call_llm(&self, messages: &[ChatMessage]) -> AgentResult<agent_client::ChatResponse> {
        let tools = self.bridge.tool_definitions();

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
        let tools_list = self.bridge
            .tool_definitions()
            .iter()
            .map(|t| format!("- {}: {}", t.name, t.description))
            .collect::<Vec<_>>()
            .join("\n");

        let prompt = format!(
            "You are an AI coding assistant.\n\n\
             Working directory: {}\n\n\
             Available tools:\n{}\n\n\
             Always explain your actions. When you have enough information, provide your answer WITHOUT calling more tools.",
            self.config.working_directory.display(),
            tools_list
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

fn truncate_output(output: &str, max_len: usize) -> String {
    if output.len() > max_len {
        format!("{}...", &output[..max_len])
    } else {
        output.to_string()
    }
}

pub fn get_tool_definitions() -> Vec<agent_client::ToolDefinition> {
    let bridge = ToolBridge::new(Config::default());
    bridge.tool_definitions()
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

fn is_file_edit_tool(tool_name: &str) -> bool {
    matches!(tool_name, "write_file" | "apply_patch" | "replace_in_file" | "insert_code_block")
}

fn format_tool_input(input: &serde_json::Value) -> String {
    if let Some(path) = input.get("path").and_then(|v| v.as_str()) {
        if let Some(cmd) = input.get("command").and_then(|v| v.as_str()) {
            return format!("path: {}, command: {}", path, truncate_output(cmd, 50));
        }
        return format!("path: {}", path);
    }
    if let Some(cmd) = input.get("command").and_then(|v| v.as_str()) {
        return format!("command: {}", truncate_output(cmd, 80));
    }
    truncate_output(&serde_json::to_string_pretty(input).unwrap_or_default(), 200)
}
