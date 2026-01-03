#![deny(clippy::all)]

use crate::approval::{ApprovalChecker, ApprovalLevel, ApprovalSettings};
use crate::cli::Args;
use crate::tool_bridge::ToolBridge;
use agent_client::providers::OpenAiProvider;
use agent_client::{ChatMessage, ChatRequest, ContentPart, MessageContent, ModelProvider};
use agent_common::AgentResult;
use agent_config::Config;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::mpsc;
use tower_http::cors::{Any, CorsLayer};

#[derive(Clone)]
struct AppState {
    provider: Arc<OpenAiProvider>,
    config: Arc<Config>,
    bridge: Arc<ToolBridge>,
    max_turns: u32,
    yolo: bool,
}

#[derive(Deserialize)]
struct ClientMessage {
    #[serde(rename = "type")]
    msg_type: String,
    #[serde(default)]
    data: serde_json::Value,
}

#[derive(Serialize)]
struct ServerMessage {
    #[serde(rename = "type")]
    msg_type: String,
    data: serde_json::Value,
}

impl ServerMessage {
    fn new(msg_type: &str, data: serde_json::Value) -> Self {
        Self { msg_type: msg_type.to_string(), data }
    }

    fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
}

pub async fn run_server(args: Args) -> AgentResult<()> {
    let api_key = args.api_key().ok_or_else(|| {
        agent_common::AgentError::configuration("API key not provided")
    })?;

    let config = build_config(&args);
    let provider_config = config.provider()?;

    let provider = OpenAiProvider::new(&api_key, config.model_id()?)
        .with_base_url(&provider_config.base_url);

    let state = AppState {
        provider: Arc::new(provider),
        config: Arc::new(config.clone()),
        bridge: Arc::new(ToolBridge::new(config)),
        max_turns: args.max_turns,
        yolo: args.yolo,
    };

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        .route("/ws", get(ws_handler))
        .layer(cors)
        .with_state(state);

    let addr = format!("0.0.0.0:{}", args.port);
    println!("WebSocket server running at ws://{}/ws", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: AppState) {
    let (mut sender, mut receiver) = socket.split();
    let (tx, mut rx) = mpsc::channel::<ServerMessage>(32);

    let send_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if sender.send(Message::Text(msg.to_json().into())).await.is_err() {
                break;
            }
        }
    });

    while let Some(Ok(msg)) = receiver.next().await {
        if let Message::Text(text) = msg {
            if let Ok(client_msg) = serde_json::from_str::<ClientMessage>(&text) {
                if client_msg.msg_type == "chat" {
                    let message = client_msg.data.get("message")
                        .and_then(|m| m.as_str())
                        .unwrap_or("")
                        .to_string();

                    if !message.is_empty() {
                        let tx_clone = tx.clone();
                        let state_clone = state.clone();
                        tokio::spawn(async move {
                            run_agent_loop(state_clone, message, tx_clone).await;
                        });
                    }
                }
            }
        }
    }

    send_task.abort();
}

async fn run_agent_loop(state: AppState, user_message: String, tx: mpsc::Sender<ServerMessage>) {
    let settings = if state.yolo {
        ApprovalSettings::yolo()
    } else {
        ApprovalSettings::default_safe()
    };
    let checker = ApprovalChecker::new(settings);

    let system = system_prompt(&state.config);
    let mut messages = vec![
        ChatMessage::system(system),
        ChatMessage::user(user_message),
    ];

    for _turn in 1..=state.max_turns {
        let tools = crate::agent_runner::get_tool_definitions();
        let request = ChatRequest {
            messages: messages.clone(),
            tools: Some(tools),
            max_tokens: Some(4096),
            temperature: Some(0.7),
            stop: None,
        };

        let response = match state.provider.chat(request).await {
            Ok(r) => r,
            Err(e) => {
                let _ = tx.send(ServerMessage::new("error", serde_json::json!(e.to_string()))).await;
                break;
            }
        };

        let tool_calls = extract_tool_calls(&response.content);
        let text = response.content.as_text().map(String::from);

        if tool_calls.is_empty() {
            let msg = text.unwrap_or_else(|| "Done".to_string());
            let _ = tx.send(ServerMessage::new("message", serde_json::json!(msg))).await;
            break;
        }

        messages.push(ChatMessage {
            role: agent_client::Role::Assistant,
            content: response.content,
        });

        if let Some(t) = text.filter(|s| !s.is_empty()) {
            let _ = tx.send(ServerMessage::new("message", serde_json::json!(t))).await;
        }

        for (id, name, input_val) in tool_calls {
            let approval_level = checker.check_tool(&name, &input_val);

            if approval_level != ApprovalLevel::Auto {
                let _ = tx.send(ServerMessage::new("approval_required", serde_json::json!({
                    "id": id,
                    "tool": name,
                    "level": format!("{:?}", approval_level),
                    "input": input_val
                }))).await;
            }

            let _ = tx.send(ServerMessage::new("tool_start", serde_json::json!({
                "id": id, "name": name, "input": input_val,
                "requires_approval": approval_level != ApprovalLevel::Auto
            }))).await;

            let result = state.bridge.execute_tool(&name, input_val).await;
            let (output, is_error) = match result {
                Ok(out) => (out, false),
                Err(e) => (e.to_string(), true),
            };

            let content = if is_error {
                format!("Error: {}", output)
            } else {
                output.clone()
            };

            messages.push(ChatMessage::tool(&id, content));

            let truncated = if output.len() > 500 {
                format!("{}...", &output[..500])
            } else {
                output
            };

            let _ = tx.send(ServerMessage::new("tool_end", serde_json::json!({
                "id": id, "output": truncated, "error": is_error
            }))).await;
        }
    }

    let _ = tx.send(ServerMessage::new("done", serde_json::json!(null))).await;
}

fn system_prompt(config: &Config) -> String {
    format!(
        "You are an AI coding assistant.\n\nWorking directory: {}\n\n\
        Available tools: read_file, write_file, replace_in_file, execute_command, \
        list_files, search_files\n\n\
        Always explain your actions.",
        config.working_directory.display()
    )
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

fn build_config(args: &Args) -> Config {
    let mut config = Config::default();
    config.provider_id = args.provider.clone();
    config.working_directory = args.working_directory();
    if let Some(ref model) = args.model {
        config.model_id = Some(model.clone());
    }
    config
}
