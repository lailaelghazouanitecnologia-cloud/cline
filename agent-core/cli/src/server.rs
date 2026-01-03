#![deny(clippy::all)]

use crate::cli::Args;
use crate::tool_bridge::ToolBridge;
use agent_client::providers::OpenAiProvider;
use agent_client::{ChatMessage, ChatRequest, ContentPart, MessageContent, ModelProvider};
use agent_common::AgentResult;
use agent_config::Config;
use axum::extract::State;
use axum::response::sse::{Event, Sse};
use axum::routing::post;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};

#[derive(Clone)]
struct AppState {
    provider: Arc<OpenAiProvider>,
    config: Arc<Config>,
    bridge: Arc<ToolBridge>,
    max_turns: u32,
}

#[derive(Deserialize)]
struct ChatInput {
    message: String,
    #[allow(dead_code)]
    #[serde(default)]
    session_id: Option<String>,
}

#[derive(Serialize)]
struct AgentEvent {
    #[serde(rename = "type")]
    event_type: String,
    data: serde_json::Value,
}

pub async fn run_server(args: Args) -> AgentResult<()> {
    let api_key = args.api_key().ok_or_else(|| {
        agent_common::AgentError::configuration("API key not provided")
    })?;

    let config = build_config(&args);
    let provider_config = config.provider()?;

    let provider = OpenAiProvider::new(&api_key, config.model_id()?)
        .with_base_url(&provider_config.base_url);

    let bridge = Arc::new(ToolBridge::new(config.clone()));

    let state = AppState {
        provider: Arc::new(provider),
        config: Arc::new(config),
        bridge,
        max_turns: args.max_turns,
    };

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        .route("/chat", post(chat_handler))
        .layer(cors)
        .with_state(state);

    let addr = format!("0.0.0.0:{}", args.port);
    println!("Server running at http://{}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn chat_handler(
    State(state): State<AppState>,
    Json(input): Json<ChatInput>,
) -> Sse<impl futures::Stream<Item = Result<Event, std::convert::Infallible>>> {
    let stream = async_stream::stream! {
        let system = system_prompt(&state.config);
        let mut messages = vec![
            ChatMessage::system(system),
            ChatMessage::user(input.message),
        ];

        for turn in 1..=state.max_turns {
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
                    yield Ok(emit_event("error", serde_json::json!(e.to_string())));
                    break;
                }
            };

            let tool_calls = extract_tool_calls(&response.content);
            let text = response.content.as_text().map(String::from);

            if tool_calls.is_empty() {
                let msg = text.unwrap_or_else(|| "Done".to_string());
                yield Ok(emit_event("message", serde_json::json!(msg)));
                break;
            }

            messages.push(ChatMessage {
                role: agent_client::Role::Assistant,
                content: response.content,
            });

            if let Some(t) = text.filter(|s| !s.is_empty()) {
                yield Ok(emit_event("message", serde_json::json!(t)));
            }

            for (id, name, input_val) in tool_calls {
                yield Ok(emit_event("tool_start", serde_json::json!({
                    "id": id, "name": name, "input": input_val
                })));

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

                yield Ok(emit_event("tool_end", serde_json::json!({
                    "id": id, "output": truncated, "error": is_error
                })));
            }
        }

        yield Ok(Event::default().data("[DONE]"));
    };

    Sse::new(stream)
}

fn emit_event(event_type: &str, data: serde_json::Value) -> Event {
    let event = AgentEvent {
        event_type: event_type.to_string(),
        data,
    };
    let json = serde_json::to_string(&event).unwrap_or_default();
    Event::default().data(json)
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
