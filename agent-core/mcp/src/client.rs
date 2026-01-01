#![deny(clippy::all)]
#![forbid(unsafe_code)]

use crate::protocol::{JsonRpcRequest, JsonRpcResponse, RequestId};
use crate::transport::Transport;
use crate::types::{
    Prompt, PromptMessage, Resource, ResourceContent, ServerCapabilities, ServerInfo, Tool,
    ToolCallRequest, ToolCallResponse,
};
use agent_common::{AgentError, AgentResult};
use serde::de::DeserializeOwned;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct McpClient<T: Transport> {
    transport: Arc<Mutex<T>>,
    request_id: AtomicI64,
    server_info: Option<ServerInfo>,
}

impl<T: Transport> McpClient<T> {
    pub fn new(transport: T) -> Self {
        Self {
            transport: Arc::new(Mutex::new(transport)),
            request_id: AtomicI64::new(1),
            server_info: None,
        }
    }

    pub async fn initialize(
        &mut self,
        client_name: &str,
        client_version: &str,
    ) -> AgentResult<ServerInfo> {
        let params = json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {
                "name": client_name,
                "version": client_version
            }
        });

        let result: InitializeResult = self.call("initialize", Some(params)).await?;

        let info = ServerInfo {
            name: result.server_info.name,
            version: result.server_info.version,
            capabilities: result.capabilities,
        };

        self.server_info = Some(info.clone());
        self.notify("notifications/initialized", None).await?;

        Ok(info)
    }

    pub async fn list_tools(&self) -> AgentResult<Vec<Tool>> {
        let result: ListToolsResult = self.call("tools/list", None).await?;
        Ok(result.tools)
    }

    pub async fn call_tool(&self, request: ToolCallRequest) -> AgentResult<ToolCallResponse> {
        let params = json!({
            "name": request.name,
            "arguments": request.arguments
        });

        let result: ToolCallResponse = self.call("tools/call", Some(params)).await?;
        Ok(result)
    }

    pub async fn list_resources(&self) -> AgentResult<Vec<Resource>> {
        let result: ListResourcesResult = self.call("resources/list", None).await?;
        Ok(result.resources)
    }

    pub async fn read_resource(&self, uri: &str) -> AgentResult<Vec<ResourceContent>> {
        let params = json!({ "uri": uri });
        let result: ReadResourceResult = self.call("resources/read", Some(params)).await?;
        Ok(result.contents)
    }

    pub async fn list_prompts(&self) -> AgentResult<Vec<Prompt>> {
        let result: ListPromptsResult = self.call("prompts/list", None).await?;
        Ok(result.prompts)
    }

    pub async fn get_prompt(
        &self,
        name: &str,
        arguments: Option<Value>,
    ) -> AgentResult<Vec<PromptMessage>> {
        let params = json!({
            "name": name,
            "arguments": arguments
        });

        let result: GetPromptResult = self.call("prompts/get", Some(params)).await?;
        Ok(result.messages)
    }

    pub async fn shutdown(&self) -> AgentResult<()> {
        let _: Value = self.call("shutdown", None).await?;
        Ok(())
    }

    pub fn server_info(&self) -> Option<&ServerInfo> {
        self.server_info.as_ref()
    }

    pub fn capabilities(&self) -> Option<&ServerCapabilities> {
        self.server_info.as_ref().map(|info| &info.capabilities)
    }

    async fn call<R: DeserializeOwned>(&self, method: &str, params: Option<Value>) -> AgentResult<R> {
        let id = self.next_id();
        let request = JsonRpcRequest::new(id, method);
        let request = if let Some(params) = params {
            request.with_params(params)
        } else {
            request
        };

        let response = self.send_request(request).await?;

        if let Some(error) = response.error {
            return Err(AgentError::mcp(format!(
                "{}: {}",
                error.code, error.message
            )));
        }

        let result = response
            .result
            .ok_or_else(|| AgentError::mcp("missing result in response"))?;

        serde_json::from_value(result)
            .map_err(|e| AgentError::mcp(format!("failed to parse result: {}", e)))
    }

    async fn notify(&self, method: &str, params: Option<Value>) -> AgentResult<()> {
        let notification = crate::protocol::JsonRpcNotification::new(method);
        let notification = if let Some(params) = params {
            notification.with_params(params)
        } else {
            notification
        };

        let message = serde_json::to_string(&notification)
            .map_err(|e| AgentError::mcp(format!("failed to serialize notification: {}", e)))?;

        let mut transport = self.transport.lock().await;
        transport
            .send(&message)
            .await
            .map_err(|e| AgentError::mcp(format!("failed to send notification: {}", e)))
    }

    async fn send_request(&self, request: JsonRpcRequest) -> AgentResult<JsonRpcResponse> {
        let message = serde_json::to_string(&request)
            .map_err(|e| AgentError::mcp(format!("failed to serialize request: {}", e)))?;

        let mut transport = self.transport.lock().await;

        transport
            .send(&message)
            .await
            .map_err(|e| AgentError::mcp(format!("failed to send request: {}", e)))?;

        let response_str = transport
            .receive()
            .await
            .map_err(|e| AgentError::mcp(format!("failed to receive response: {}", e)))?;

        serde_json::from_str(&response_str)
            .map_err(|e| AgentError::mcp(format!("failed to parse response: {}", e)))
    }

    fn next_id(&self) -> RequestId {
        RequestId::Number(self.request_id.fetch_add(1, Ordering::SeqCst))
    }
}

#[derive(Debug, serde::Deserialize)]
struct InitializeResult {
    #[serde(rename = "serverInfo")]
    server_info: ServerInfoResult,
    #[serde(default)]
    capabilities: ServerCapabilities,
}

#[derive(Debug, serde::Deserialize)]
struct ServerInfoResult {
    name: String,
    version: String,
}

#[derive(Debug, serde::Deserialize)]
struct ListToolsResult {
    tools: Vec<Tool>,
}

#[derive(Debug, serde::Deserialize)]
struct ListResourcesResult {
    resources: Vec<Resource>,
}

#[derive(Debug, serde::Deserialize)]
struct ReadResourceResult {
    contents: Vec<ResourceContent>,
}

#[derive(Debug, serde::Deserialize)]
struct ListPromptsResult {
    prompts: Vec<Prompt>,
}

#[derive(Debug, serde::Deserialize)]
struct GetPromptResult {
    messages: Vec<PromptMessage>,
}
