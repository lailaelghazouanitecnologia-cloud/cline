#![deny(clippy::all)]

use crate::approval::{ApprovalChecker, ApprovalLevel, ApprovalSettings};
use crate::cli::Args;
use crate::context::ContextManager;
use crate::slash_commands::{CommandRegistry, SlashCommandParser};
use crate::store::SessionStore;
use crate::tool_bridge::ToolBridge;
use crate::usage::{RequestUsage, UsageTracker};
use agent_client::providers::OpenAiProvider;
use agent_client::{
    ChatMessage, ChatRequest, ContentPart, DeltaType, MessageContent, ModelProvider,
    StreamBuffer, StreamEventType,
};
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
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::interval;
use tower_http::cors::{Any, CorsLayer};

const PROVIDER_URLS: &[(&str, &str)] = &[
    ("groq", "https://api.groq.com/openai/v1"),
    ("openai", "https://api.openai.com/v1"),
    ("anthropic", "https://api.anthropic.com"),
];

const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(30);
const MAX_RETRIES: u32 = 3;
const RETRY_DELAY_MS: u64 = 1000;

#[derive(Clone)]
struct AppState {
    config: Arc<Config>,
    bridge: Arc<ToolBridge>,
    store: Arc<SessionStore>,
    context_manager: ContextManager,
    usage_tracker: UsageTracker,
    command_registry: Arc<CommandRegistry>,
    command_parser: Arc<SlashCommandParser>,
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

#[derive(Serialize, Clone)]
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

#[derive(Clone, Serialize)]
struct ApiError {
    code: String,
    message: String,
    provider: Option<String>,
    model: Option<String>,
    is_rate_limit: bool,
    is_retryable: bool,
}

impl ApiError {
    fn from_error(e: &agent_common::AgentError, provider: Option<&str>, model: Option<&str>) -> Self {
        let msg = e.to_string();
        let is_rate_limit = msg.contains("429")
            || msg.to_lowercase().contains("rate limit")
            || msg.to_lowercase().contains("quota exceeded")
            || msg.to_lowercase().contains("too many requests");

        let is_retryable = is_rate_limit
            || msg.contains("500")
            || msg.contains("502")
            || msg.contains("503")
            || msg.to_lowercase().contains("timeout");

        Self {
            code: if is_rate_limit { "rate_limit".into() } else { "api_error".into() },
            message: msg,
            provider: provider.map(String::from),
            model: model.map(String::from),
            is_rate_limit,
            is_retryable,
        }
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
        context_manager: ContextManager::new(),
        usage_tracker: UsageTracker::new(),
        command_registry: Arc::new(CommandRegistry::new()),
        command_parser: Arc::new(SlashCommandParser::new()),
        max_turns: args.max_turns,
        yolo: args.yolo,
    };

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        .route("/ws", get(ws_handler))
        .route("/health", get(health_check))
        .route("/api/sessions", get(list_sessions))
        .route("/api/sessions/{id}", get(get_session))
        .route("/api/sessions/{id}/messages", get(get_messages))
        .route("/api/sessions/{id}/context", get(get_session_context))
        .route("/api/sessions/{id}/usage", get(get_session_usage))
        .route("/api/context/active", get(list_active_contexts))
        .route("/api/usage/total", get(get_total_usage))
        .layer(cors)
        .with_state(state);

    let addr = format!("0.0.0.0:{}", args.port);
    println!("Server running at http://{}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn health_check() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "status": "ok", "timestamp": chrono::Utc::now().to_rfc3339() }))
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

async fn get_session_context(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Json<serde_json::Value> {
    match state.context_manager.get_context(&id) {
        Some(ctx) => Json(serde_json::json!({ "context": ctx })),
        None => Json(serde_json::json!({ "error": "Context not found" })),
    }
}

async fn list_active_contexts(State(state): State<AppState>) -> Json<serde_json::Value> {
    let contexts = state.context_manager.list_active();
    Json(serde_json::json!({ "contexts": contexts }))
}

async fn get_session_usage(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Json<serde_json::Value> {
    match state.usage_tracker.get_session_usage(&id) {
        Some(usage) => Json(serde_json::json!({ "usage": usage })),
        None => Json(serde_json::json!({ "error": "No usage data for session" })),
    }
}

async fn get_total_usage(State(state): State<AppState>) -> Json<serde_json::Value> {
    let summaries = state.usage_tracker.get_all_summaries();
    let total_cost = state.usage_tracker.get_total_cost();
    Json(serde_json::json!({ "sessions": summaries, "total_cost_usd": total_cost }))
}

async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: AppState) {
    let (mut sender, mut receiver) = socket.split();
    let (tx, mut rx) = mpsc::channel::<ServerMessage>(64);
    let (ping_tx, mut ping_rx) = mpsc::channel::<()>(1);

    let send_task = tokio::spawn(async move {
        let mut heartbeat = interval(HEARTBEAT_INTERVAL);
        loop {
            tokio::select! {
                Some(msg) = rx.recv() => {
                    if sender.send(Message::Text(msg.to_json().into())).await.is_err() {
                        break;
                    }
                }
                _ = heartbeat.tick() => {
                    if sender.send(Message::Ping(vec![].into())).await.is_err() {
                        break;
                    }
                }
                _ = ping_rx.recv() => {
                    if sender.send(Message::Pong(vec![].into())).await.is_err() {
                        break;
                    }
                }
                else => break,
            }
        }
    });

    let store = state.store.clone();
    let _ = tx.send(ServerMessage::new("connected", serde_json::json!({
        "timestamp": chrono::Utc::now().to_rfc3339()
    }))).await;

    let _ = tx.send(ServerMessage::new("sessions", match store.list_sessions(20) {
        Ok(sessions) => serde_json::json!(sessions),
        Err(_) => serde_json::json!([]),
    })).await;

    while let Some(result) = receiver.next().await {
        match result {
            Ok(Message::Text(text)) => {
                if let Ok(client_msg) = serde_json::from_str::<ClientMessage>(&text) {
                    handle_client_message(&state, &tx, client_msg).await;
                }
            }
            Ok(Message::Ping(_)) => {
                let _ = ping_tx.send(()).await;
            }
            Ok(Message::Pong(_)) => {}
            Ok(Message::Close(_)) => break,
            Err(_) => break,
            _ => {}
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
        "ping" => {
            let _ = tx.send(ServerMessage::new("pong", serde_json::json!({
                "timestamp": chrono::Utc::now().to_rfc3339()
            }))).await;
        }
        "chat" => handle_chat(state, tx, &msg.data).await,
        "resume_session" => handle_resume_session(state, tx, &msg.data).await,
        "load_session" => {
            let session_id = msg.data.get("sessionId").and_then(|s| s.as_str()).unwrap_or("");
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
            let session_id = msg.data.get("sessionId").and_then(|s| s.as_str()).unwrap_or("");
            if state.store.delete_session(session_id).is_ok() {
                let _ = tx.send(ServerMessage::new("session_deleted", serde_json::json!(session_id))).await;
            }
        }
        _ => {}
    }
}

async fn handle_chat(state: &AppState, tx: &mpsc::Sender<ServerMessage>, data: &serde_json::Value) {
    let message = data.get("message").and_then(|m| m.as_str()).unwrap_or("").to_string();
    let session_id = data.get("sessionId").and_then(|s| s.as_str()).map(String::from);
    let api_key = data.get("apiKey").and_then(|k| k.as_str()).map(String::from);
    let provider_id = data.get("providerId").and_then(|p| p.as_str()).unwrap_or("groq").to_string();
    let model_id = data.get("modelId").and_then(|m| m.as_str()).unwrap_or("llama-3.3-70b-versatile").to_string();

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
            run_agent_loop(state_clone, message, session_id, api_key, provider_id, model_id, tx_clone).await;
        });
    }
}

async fn handle_resume_session(state: &AppState, tx: &mpsc::Sender<ServerMessage>, data: &serde_json::Value) {
    let session_id = match data.get("sessionId").and_then(|s| s.as_str()) {
        Some(id) => id.to_string(),
        None => {
            let _ = tx.send(ServerMessage::new("error", serde_json::json!({
                "code": "invalid_request",
                "message": "sessionId is required"
            }))).await;
            return;
        }
    };

    let api_key = data.get("apiKey").and_then(|k| k.as_str()).map(String::from);
    let provider_id = data.get("providerId").and_then(|p| p.as_str()).unwrap_or("groq").to_string();
    let model_id = data.get("modelId").and_then(|m| m.as_str()).unwrap_or("llama-3.3-70b-versatile").to_string();
    let new_message = data.get("message").and_then(|m| m.as_str()).unwrap_or("").to_string();

    if api_key.is_none() || api_key.as_ref().map(|k| k.is_empty()).unwrap_or(true) {
        let _ = tx.send(ServerMessage::new("api_key_required", serde_json::json!({
            "message": "API key is required"
        }))).await;
        return;
    }

    let session = match state.store.get_session(&session_id) {
        Ok(Some(s)) => s,
        _ => {
            let _ = tx.send(ServerMessage::new("error", serde_json::json!({
                "code": "session_not_found",
                "message": "Session not found"
            }))).await;
            return;
        }
    };

    let stored_messages = match state.store.get_messages(&session_id) {
        Ok(msgs) => msgs,
        Err(_) => {
            let _ = tx.send(ServerMessage::new("error", serde_json::json!({
                "code": "load_error",
                "message": "Failed to load session messages"
            }))).await;
            return;
        }
    };

    let _ = tx.send(ServerMessage::new("session_resumed", serde_json::json!({
        "id": session.id,
        "title": session.title,
        "message_count": stored_messages.len()
    }))).await;

    if new_message.is_empty() {
        return;
    }

    let tx_clone = tx.clone();
    let state_clone = state.clone();
    let api_key = api_key.unwrap();

    tokio::spawn(async move {
        resume_agent_loop(state_clone, session_id, stored_messages, new_message, api_key, provider_id, model_id, tx_clone).await;
    });
}

fn get_provider_url(provider_id: &str) -> &'static str {
    PROVIDER_URLS.iter()
        .find(|(id, _)| *id == provider_id)
        .map(|(_, url)| *url)
        .unwrap_or("https://api.groq.com/openai/v1")
}

async fn handle_local_command(
    cmd: &str,
    state: &AppState,
    tx: &mpsc::Sender<ServerMessage>,
    session_id: &str,
) -> bool {
    match cmd {
        "help" => {
            let commands = state.command_registry.all_commands();
            let help_data: Vec<_> = commands
                .iter()
                .map(|c| serde_json::json!({
                    "name": c.name,
                    "description": c.description
                }))
                .collect();
            let _ = tx.send(ServerMessage::new("help", serde_json::json!({
                "commands": help_data
            }))).await;
            true
        }
        "clear" => {
            let _ = tx.send(ServerMessage::new("clear", serde_json::json!({
                "session_id": session_id
            }))).await;
            true
        }
        "history" => {
            if let Ok(sessions) = state.store.list_sessions(20) {
                let _ = tx.send(ServerMessage::new("history", serde_json::json!({
                    "sessions": sessions
                }))).await;
            }
            true
        }
        "usage" => {
            if let Some(usage) = state.usage_tracker.get_session_summary(session_id) {
                let _ = tx.send(ServerMessage::new("usage_info", serde_json::json!({
                    "total_tokens": usage.total_tokens,
                    "total_cost_usd": usage.total_cost_usd,
                    "request_count": usage.request_count
                }))).await;
            } else {
                let _ = tx.send(ServerMessage::new("usage_info", serde_json::json!({
                    "total_tokens": 0,
                    "total_cost_usd": 0.0,
                    "request_count": 0
                }))).await;
            }
            true
        }
        "yolo" => {
            let _ = tx.send(ServerMessage::new("yolo_toggle", serde_json::json!({
                "message": "YOLO mode toggled. Reload to apply."
            }))).await;
            true
        }
        "debug" => {
            let _ = tx.send(ServerMessage::new("debug_toggle", serde_json::json!({
                "message": "Debug mode toggled"
            }))).await;
            true
        }
        _ => false,
    }
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

    let parsed_cmd = state.command_parser.parse(&user_message, &state.command_registry);

    if let Some(ref cmd) = parsed_cmd {
        let _ = tx.send(ServerMessage::new("slash_command", serde_json::json!({
            "name": cmd.name,
            "text": cmd.text_without_command
        }))).await;

        if handle_local_command(&cmd.name, &state, &tx, &session_id).await {
            let _ = tx.send(ServerMessage::new("done", serde_json::json!(null))).await;
            return;
        }
    }

    let processed_message = match parsed_cmd {
        Some(cmd) => format!("{}{}", cmd.instruction, cmd.text_without_command),
        None => user_message.clone(),
    };

    let base_url = get_provider_url(&provider_id);
    let provider = OpenAiProvider::new(&api_key, &model_id).with_base_url(base_url);

    if state.store.get_session(&session_id).ok().flatten().is_none() {
        let _ = state.store.create_session(&session_id, &title, &provider_id, &model_id);
    }

    state.context_manager.create_context(
        &session_id,
        state.config.working_directory.clone(),
        &provider_id,
        &model_id,
    );

    let msg_id = uuid::Uuid::new_v4().to_string();
    let _ = state.store.add_message(&msg_id, &session_id, "user", &user_message, None);

    let _ = tx.send(ServerMessage::new("session_created", serde_json::json!({
        "id": session_id,
        "title": title
    }))).await;

    let settings = if state.yolo { ApprovalSettings::yolo() } else { ApprovalSettings::default_safe() };
    let checker = ApprovalChecker::new(settings);

    let system = system_prompt(&state.config, &state.bridge);
    let mut messages = vec![ChatMessage::system(system), ChatMessage::user(&processed_message)];

    let session_id_clone = session_id.clone();

    for turn in 1..=state.max_turns {
        state.context_manager.update_context(&session_id_clone, |ctx| ctx.increment_turn());
        let _ = tx.send(ServerMessage::new("turn_start", serde_json::json!({
            "turn": turn, "max_turns": state.max_turns
        }))).await;

        let tools = state.bridge.tool_definitions();
        let request = ChatRequest {
            messages: messages.clone(),
            tools: Some(tools),
            max_tokens: Some(4096),
            temperature: Some(0.7),
            stop: None,
        };

        let stream_result = call_with_retry(&provider, request, &provider_id, &model_id, &tx).await;
        let mut stream = match stream_result {
            Ok(s) => s,
            Err(_) => break,
        };

        let mut buffer = StreamBuffer::default();

        while let Some(event_result) = stream.next().await {
            match event_result {
                Ok(event) => {
                    buffer.process_event(&event);
                    if let Some(delta) = extract_text_delta(&event.event_type) {
                        let _ = tx.send(ServerMessage::new("text_delta", serde_json::json!({
                            "delta": delta
                        }))).await;
                    }
                }
                Err(e) => {
                    let api_error = ApiError::from_error(&e, Some(&provider_id), Some(&model_id));
                    let _ = tx.send(ServerMessage::new("error", serde_json::to_value(&api_error).unwrap())).await;
                    break;
                }
            }
        }

        let usage = RequestUsage::new(buffer.usage.prompt_tokens, buffer.usage.completion_tokens);
        state.usage_tracker.record_usage(&session_id, &model_id, &provider_id, usage.clone(), turn);
        let _ = tx.send(ServerMessage::new("usage", serde_json::json!({
            "turn": turn,
            "input_tokens": usage.input_tokens,
            "output_tokens": usage.output_tokens,
            "total_tokens": usage.total_tokens
        }))).await;

        let content = buffer.into_content();
        let tool_calls = extract_tool_calls(&content);
        let text = content.as_text().map(String::from);

        if tool_calls.is_empty() {
            let msg = text.unwrap_or_else(|| "Done".to_string());
            let msg_id = uuid::Uuid::new_v4().to_string();
            let _ = state.store.add_message(&msg_id, &session_id, "assistant", &msg, None);
            let _ = tx.send(ServerMessage::new("message", serde_json::json!(msg))).await;
            break;
        }

        messages.push(ChatMessage { role: agent_client::Role::Assistant, content });

        if let Some(ref t) = text {
            if !t.is_empty() {
                let _ = tx.send(ServerMessage::new("message", serde_json::json!(t))).await;
            }
        }

        let mut tool_results = Vec::new();

        for (id, name, input_val) in tool_calls {
            let tool_result = execute_tool_with_retry(&state, &checker, &id, &name, &input_val, &tx).await;

            let (output, is_error) = match tool_result {
                Ok(out) => (out, false),
                Err(e) => (e, true),
            };

            let content = if is_error { format!("Error: {}", output) } else { output.clone() };
            messages.push(ChatMessage::tool(&id, &content));

            tool_results.push(serde_json::json!({
                "id": id, "name": name, "input": input_val, "output": output, "error": is_error
            }));
        }

        let msg_id = uuid::Uuid::new_v4().to_string();
        let tool_calls_json = serde_json::to_string(&tool_results).ok();
        let _ = state.store.add_message(&msg_id, &session_id, "assistant", text.as_deref().unwrap_or(""), tool_calls_json.as_deref());
    }

    if let Some(ctx) = state.context_manager.get_context(&session_id) {
        let _ = tx.send(ServerMessage::new("context_summary", serde_json::to_value(ctx.to_summary()).unwrap())).await;
    }

    if let Some(usage_summary) = state.usage_tracker.get_session_summary(&session_id) {
        let _ = tx.send(ServerMessage::new("usage_summary", serde_json::json!({
            "total_tokens": usage_summary.total_tokens,
            "total_cost_usd": usage_summary.total_cost_usd,
            "request_count": usage_summary.request_count
        }))).await;
    }

    let _ = tx.send(ServerMessage::new("done", serde_json::json!(null))).await;
}

async fn resume_agent_loop(
    state: AppState,
    session_id: String,
    stored_messages: Vec<crate::store::StoredMessage>,
    new_message: String,
    api_key: String,
    provider_id: String,
    model_id: String,
    tx: mpsc::Sender<ServerMessage>,
) {
    let base_url = get_provider_url(&provider_id);
    let provider = OpenAiProvider::new(&api_key, &model_id).with_base_url(base_url);

    let settings = if state.yolo { ApprovalSettings::yolo() } else { ApprovalSettings::default_safe() };
    let checker = ApprovalChecker::new(settings);

    let system = system_prompt(&state.config, &state.bridge);
    let mut messages = vec![ChatMessage::system(system)];

    for stored in &stored_messages {
        match stored.role.as_str() {
            "user" => messages.push(ChatMessage::user(&stored.content)),
            "assistant" => messages.push(ChatMessage {
                role: agent_client::Role::Assistant,
                content: agent_client::MessageContent::Text(stored.content.clone()),
            }),
            _ => {}
        }
    }

    let msg_id = uuid::Uuid::new_v4().to_string();
    let _ = state.store.add_message(&msg_id, &session_id, "user", &new_message, None);
    messages.push(ChatMessage::user(&new_message));

    let _ = tx.send(ServerMessage::new("session_continue", serde_json::json!({
        "session_id": session_id,
        "restored_messages": stored_messages.len()
    }))).await;

    for turn in 1..=state.max_turns {
        let _ = tx.send(ServerMessage::new("turn_start", serde_json::json!({
            "turn": turn, "max_turns": state.max_turns
        }))).await;

        let tools = state.bridge.tool_definitions();
        let request = ChatRequest {
            messages: messages.clone(),
            tools: Some(tools),
            max_tokens: Some(4096),
            temperature: Some(0.7),
            stop: None,
        };

        let stream_result = call_with_retry(&provider, request, &provider_id, &model_id, &tx).await;
        let mut stream = match stream_result {
            Ok(s) => s,
            Err(_) => break,
        };

        let mut buffer = StreamBuffer::default();

        while let Some(event_result) = stream.next().await {
            match event_result {
                Ok(event) => {
                    buffer.process_event(&event);
                    if let Some(delta) = extract_text_delta(&event.event_type) {
                        let _ = tx.send(ServerMessage::new("text_delta", serde_json::json!({
                            "delta": delta
                        }))).await;
                    }
                }
                Err(e) => {
                    let api_error = ApiError::from_error(&e, Some(&provider_id), Some(&model_id));
                    let _ = tx.send(ServerMessage::new("error", serde_json::to_value(&api_error).unwrap())).await;
                    break;
                }
            }
        }

        let usage = RequestUsage::new(buffer.usage.prompt_tokens, buffer.usage.completion_tokens);
        state.usage_tracker.record_usage(&session_id, &model_id, &provider_id, usage.clone(), turn);
        let _ = tx.send(ServerMessage::new("usage", serde_json::json!({
            "turn": turn,
            "input_tokens": usage.input_tokens,
            "output_tokens": usage.output_tokens,
            "total_tokens": usage.total_tokens
        }))).await;

        let content = buffer.into_content();
        let tool_calls = extract_tool_calls(&content);
        let text = content.as_text().map(String::from);

        if tool_calls.is_empty() {
            let msg = text.unwrap_or_else(|| "Done".to_string());
            let msg_id = uuid::Uuid::new_v4().to_string();
            let _ = state.store.add_message(&msg_id, &session_id, "assistant", &msg, None);
            let _ = tx.send(ServerMessage::new("message", serde_json::json!(msg))).await;
            break;
        }

        messages.push(ChatMessage { role: agent_client::Role::Assistant, content });

        if let Some(ref t) = text {
            if !t.is_empty() {
                let _ = tx.send(ServerMessage::new("message", serde_json::json!(t))).await;
            }
        }

        let mut tool_results = Vec::new();

        for (id, name, input_val) in tool_calls {
            let tool_result = execute_tool_with_retry(&state, &checker, &id, &name, &input_val, &tx).await;

            let (output, is_error) = match tool_result {
                Ok(out) => (out, false),
                Err(e) => (e, true),
            };

            let content = if is_error { format!("Error: {}", output) } else { output.clone() };
            messages.push(ChatMessage::tool(&id, &content));

            tool_results.push(serde_json::json!({
                "id": id, "name": name, "input": input_val, "output": output, "error": is_error
            }));
        }

        let msg_id = uuid::Uuid::new_v4().to_string();
        let tool_calls_json = serde_json::to_string(&tool_results).ok();
        let _ = state.store.add_message(&msg_id, &session_id, "assistant", text.as_deref().unwrap_or(""), tool_calls_json.as_deref());
    }

    if let Some(ctx) = state.context_manager.get_context(&session_id) {
        let _ = tx.send(ServerMessage::new("context_summary", serde_json::to_value(ctx.to_summary()).unwrap())).await;
    }

    if let Some(usage_summary) = state.usage_tracker.get_session_summary(&session_id) {
        let _ = tx.send(ServerMessage::new("usage_summary", serde_json::json!({
            "total_tokens": usage_summary.total_tokens,
            "total_cost_usd": usage_summary.total_cost_usd,
            "request_count": usage_summary.request_count
        }))).await;
    }

    let _ = tx.send(ServerMessage::new("done", serde_json::json!(null))).await;
}

async fn call_with_retry(
    provider: &OpenAiProvider,
    request: ChatRequest,
    provider_id: &str,
    model_id: &str,
    tx: &mpsc::Sender<ServerMessage>,
) -> Result<impl futures::Stream<Item = AgentResult<agent_client::StreamEvent>>, ()> {
    for attempt in 0..MAX_RETRIES {
        match provider.chat_stream(request.clone()).await {
            Ok(stream) => return Ok(stream),
            Err(e) => {
                let api_error = ApiError::from_error(&e, Some(provider_id), Some(model_id));

                if !api_error.is_retryable || attempt == MAX_RETRIES - 1 {
                    let _ = tx.send(ServerMessage::new("error", serde_json::to_value(&api_error).unwrap())).await;
                    return Err(());
                }

                let delay = RETRY_DELAY_MS * (2_u64.pow(attempt));
                let _ = tx.send(ServerMessage::new("retry", serde_json::json!({
                    "attempt": attempt + 1,
                    "max_retries": MAX_RETRIES,
                    "delay_ms": delay,
                    "reason": api_error.message
                }))).await;

                tokio::time::sleep(Duration::from_millis(delay)).await;
            }
        }
    }
    Err(())
}

async fn execute_tool_with_retry(
    state: &AppState,
    checker: &ApprovalChecker,
    id: &str,
    name: &str,
    input_val: &serde_json::Value,
    tx: &mpsc::Sender<ServerMessage>,
) -> Result<String, String> {
    let approval_level = checker.check_tool(name, input_val);

    if approval_level != ApprovalLevel::Auto {
        let _ = tx.send(ServerMessage::new("approval_required", serde_json::json!({
            "id": id, "tool": name, "level": format!("{:?}", approval_level), "input": input_val
        }))).await;
    }

    let _ = tx.send(ServerMessage::new("tool_start", serde_json::json!({
        "id": id, "name": name, "input": input_val,
        "requires_approval": approval_level != ApprovalLevel::Auto
    }))).await;

    let mut last_error = String::new();

    for attempt in 0..MAX_RETRIES {
        match state.bridge.execute_tool(name, input_val.clone()).await {
            Ok(output) => {
                let truncated = if output.len() > 500 {
                    format!("{}...", &output[..500])
                } else {
                    output.clone()
                };
                let _ = tx.send(ServerMessage::new("tool_end", serde_json::json!({
                    "id": id, "output": truncated, "error": false
                }))).await;
                return Ok(output);
            }
            Err(e) => {
                last_error = e.to_string();

                if attempt < MAX_RETRIES - 1 {
                    let delay = RETRY_DELAY_MS * (2_u64.pow(attempt));
                    let _ = tx.send(ServerMessage::new("tool_retry", serde_json::json!({
                        "id": id, "name": name, "attempt": attempt + 1, "delay_ms": delay
                    }))).await;
                    tokio::time::sleep(Duration::from_millis(delay)).await;
                }
            }
        }
    }

    let _ = tx.send(ServerMessage::new("tool_end", serde_json::json!({
        "id": id, "output": last_error.clone(), "error": true
    }))).await;

    Err(last_error)
}

fn extract_text_delta(event_type: &StreamEventType) -> Option<String> {
    if let StreamEventType::ContentBlockDelta { delta } = event_type {
        if let DeltaType::TextDelta { text } = &delta.delta_type {
            return Some(text.clone());
        }
    }
    None
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
