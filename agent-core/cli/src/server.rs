#![deny(clippy::all)]

use crate::approval::{ApprovalChecker, ApprovalLevel, ApprovalSettings};
use crate::cli::Args;
use crate::store::SessionStore;
use crate::tool_bridge::ToolBridge;
use agent_client::providers::OpenAiProvider;
use agent_client::{ChatMessage, ChatRequest, ContentPart, MessageContent, ModelProvider};
use agent_common::AgentResult;
use agent_config::Config;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::mpsc;
use tower_http::cors::{Any, CorsLayer};

const PROVIDER_URLS: &[(&str, &str)] = &[
    ("groq", "https://api.groq.com/openai/v1"),
    ("openai", "https://api.openai.com/v1"),
    ("anthropic", "https://api.anthropic.com"),
];

#[derive(Clone)]
struct AppState {
    config: Arc<Config>,
    bridge: Arc<ToolBridge>,
    store: Arc<SessionStore>,
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
    let config = build_config(&args);

    let store = SessionStore::new().map_err(|e| {
        agent_common::AgentError::configuration(format!("Failed to init store: {}", e))
    })?;

    let state = AppState {
        config: Arc::new(config.clone()),
        bridge: Arc::new(ToolBridge::new(config)),
        store: Arc::new(store),
        max_turns: args.max_turns,
        yolo: args.yolo,
    };

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        .route("/ws", get(ws_handler))
        .route("/api/sessions", get(list_sessions))
        .route("/api/sessions/{id}", get(get_session))
        .route("/api/sessions/{id}/messages", get(get_messages))
        .layer(cors)
        .with_state(state);

    let addr = format!("0.0.0.0:{}", args.port);
    println!("Server running at http://{}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn list_sessions(State(state): State<AppState>) -> Json<serde_json::Value> {
    match state.store.list_sessions(50) {
        Ok(sessions) => Json(serde_json::json!({ "sessions": sessions })),
        Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
    }
}

async fn get_session(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Json<serde_json::Value> {
    match state.store.get_session(&id) {
        Ok(Some(session)) => Json(serde_json::json!({ "session": session })),
        Ok(None) => Json(serde_json::json!({ "error": "Session not found" })),
        Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
    }
}

async fn get_messages(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Json<serde_json::Value> {
    match state.store.get_messages(&id) {
        Ok(messages) => Json(serde_json::json!({ "messages": messages })),
        Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
    }
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

    let store = state.store.clone();
    let _ = tx.send(ServerMessage::new("sessions", match store.list_sessions(20) {
        Ok(sessions) => serde_json::json!(sessions),
        Err(_) => serde_json::json!([]),
    })).await;

    while let Some(Ok(msg)) = receiver.next().await {
        if let Message::Text(text) = msg {
            if let Ok(client_msg) = serde_json::from_str::<ClientMessage>(&text) {
                handle_client_message(&state, &tx, client_msg).await;
            }
        }
    }

    send_task.abort();
}

async fn handle_client_message(
    state: &AppState,
    tx: &mpsc::Sender<ServerMessage>,
    msg: ClientMessage,
) {
    match msg.msg_type.as_str() {
        "chat" => {
            let message = msg.data.get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("")
                .to_string();

            let session_id = msg.data.get("sessionId")
                .and_then(|s| s.as_str())
                .map(String::from);

            let api_key = msg.data.get("apiKey")
                .and_then(|k| k.as_str())
                .map(String::from);

            let provider_id = msg.data.get("providerId")
                .and_then(|p| p.as_str())
                .unwrap_or("groq")
                .to_string();

            let model_id = msg.data.get("modelId")
                .and_then(|m| m.as_str())
                .unwrap_or("llama-3.3-70b-versatile")
                .to_string();

            if api_key.is_none() || api_key.as_ref().map(|k| k.is_empty()).unwrap_or(true) {
                let _ = tx.send(ServerMessage::new("api_key_required", serde_json::json!({
                    "message": "API key is required"
                }))).await;
                return;
            }

            if !message.is_empty() {
                let tx_clone = tx.clone();
                let state_clone = state.clone();
                let api_key = api_key.unwrap();
                tokio::spawn(async move {
                    run_agent_loop(
                        state_clone,
                        message,
                        session_id,
                        api_key,
                        provider_id,
                        model_id,
                        tx_clone,
                    ).await;
                });
            }
        }
        "load_session" => {
            let session_id = msg.data.get("sessionId")
                .and_then(|s| s.as_str())
                .unwrap_or("");

            if let Ok(messages) = state.store.get_messages(session_id) {
                let _ = tx.send(ServerMessage::new("session_messages", serde_json::json!(messages))).await;
            }
        }
        "list_sessions" => {
            if let Ok(sessions) = state.store.list_sessions(50) {
                let _ = tx.send(ServerMessage::new("sessions", serde_json::json!(sessions))).await;
            }
        }
        "delete_session" => {
            let session_id = msg.data.get("sessionId")
                .and_then(|s| s.as_str())
                .unwrap_or("");

            if state.store.delete_session(session_id).is_ok() {
                let _ = tx.send(ServerMessage::new("session_deleted", serde_json::json!(session_id))).await;
            }
        }
        _ => {}
    }
}

fn get_provider_url(provider_id: &str) -> &'static str {
    PROVIDER_URLS.iter()
        .find(|(id, _)| *id == provider_id)
        .map(|(_, url)| *url)
        .unwrap_or("https://api.groq.com/openai/v1")
}

async fn run_agent_loop(
    state: AppState,
    user_message: String,
    session_id: Option<String>,
    api_key: String,
    provider_id: String,
    model_id: String,
    tx: mpsc::Sender<ServerMessage>,
) {
    let session_id = session_id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let title = user_message.chars().take(50).collect::<String>();

    let base_url = get_provider_url(&provider_id);
    let provider = OpenAiProvider::new(&api_key, &model_id).with_base_url(base_url);

    if state.store.get_session(&session_id).ok().flatten().is_none() {
        let _ = state.store.create_session(&session_id, &title, &provider_id, &model_id);
    }

    let msg_id = uuid::Uuid::new_v4().to_string();
    let _ = state.store.add_message(&msg_id, &session_id, "user", &user_message, None);

    let _ = tx.send(ServerMessage::new("session_created", serde_json::json!({
        "id": session_id,
        "title": title
    }))).await;

    let settings = if state.yolo {
        ApprovalSettings::yolo()
    } else {
        ApprovalSettings::default_safe()
    };
    let checker = ApprovalChecker::new(settings);

    let system = system_prompt(&state.config, &state.bridge);
    let mut messages = vec![
        ChatMessage::system(system),
        ChatMessage::user(&user_message),
    ];

    for _turn in 1..=state.max_turns {
        let tools = state.bridge.tool_definitions();
        let request = ChatRequest {
            messages: messages.clone(),
            tools: Some(tools),
            max_tokens: Some(4096),
            temperature: Some(0.7),
            stop: None,
        };

        let response = match provider.chat(request).await {
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

            let msg_id = uuid::Uuid::new_v4().to_string();
            let _ = state.store.add_message(&msg_id, &session_id, "assistant", &msg, None);

            let _ = tx.send(ServerMessage::new("message", serde_json::json!(msg))).await;
            break;
        }

        messages.push(ChatMessage {
            role: agent_client::Role::Assistant,
            content: response.content,
        });

        if let Some(ref t) = text {
            if !t.is_empty() {
                let _ = tx.send(ServerMessage::new("message", serde_json::json!(t))).await;
            }
        }

        let mut tool_results = Vec::new();

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

            let result = state.bridge.execute_tool(&name, input_val.clone()).await;
            let (output, is_error) = match result {
                Ok(out) => (out, false),
                Err(e) => (e.to_string(), true),
            };

            let content = if is_error {
                format!("Error: {}", output)
            } else {
                output.clone()
            };

            messages.push(ChatMessage::tool(&id, &content));

            tool_results.push(serde_json::json!({
                "id": id,
                "name": name,
                "input": input_val,
                "output": output,
                "error": is_error
            }));

            let truncated = if output.len() > 500 {
                format!("{}...", &output[..500])
            } else {
                output
            };

            let _ = tx.send(ServerMessage::new("tool_end", serde_json::json!({
                "id": id, "output": truncated, "error": is_error
            }))).await;
        }

        let msg_id = uuid::Uuid::new_v4().to_string();
        let tool_calls_json = serde_json::to_string(&tool_results).ok();
        let _ = state.store.add_message(
            &msg_id,
            &session_id,
            "assistant",
            text.as_deref().unwrap_or(""),
            tool_calls_json.as_deref(),
        );
    }

    let _ = tx.send(ServerMessage::new("done", serde_json::json!(null))).await;
}

fn system_prompt(config: &Config, bridge: &ToolBridge) -> String {
    let tools_list = bridge
        .tool_definitions()
        .iter()
        .map(|t| format!("- {}: {}", t.name, t.description))
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        "You are an AI coding assistant.\n\n\
         Working directory: {}\n\n\
         Available tools:\n{}\n\n\
         Always explain your actions.",
        config.working_directory.display(),
        tools_list
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
