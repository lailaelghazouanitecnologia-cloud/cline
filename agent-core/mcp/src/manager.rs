#![deny(clippy::all)]
#![forbid(unsafe_code)]

use crate::client::McpClient;
use crate::transport::StdioTransport;
use crate::types::{Resource, ServerInfo, Tool, ToolCallRequest, ToolCallResponse, ToolContent};
use agent_common::{AgentError, AgentResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::process::{Child, Command};
use tokio::sync::RwLock;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    pub env: HashMap<String, String>,
    pub working_dir: Option<PathBuf>,
    pub enabled: bool,
    pub auto_approve: Vec<String>,
}

impl McpServerConfig {
    pub fn new(name: impl Into<String>, command: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            command: command.into(),
            args: Vec::new(),
            env: HashMap::new(),
            working_dir: None,
            enabled: true,
            auto_approve: Vec::new(),
        }
    }

    pub fn with_args(mut self, args: Vec<String>) -> Self {
        self.args = args;
        self
    }

    pub fn with_env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.insert(key.into(), value.into());
        self
    }
}

struct McpServerInstance {
    config: McpServerConfig,
    process: Child,
    client: McpClient<StdioTransport>,
    info: ServerInfo,
    tools: Vec<Tool>,
    resources: Vec<Resource>,
}

pub struct McpServerManager {
    servers: Arc<RwLock<HashMap<String, McpServerInstance>>>,
    tool_index: Arc<RwLock<HashMap<String, String>>>,
}

impl McpServerManager {
    pub fn new() -> Self {
        Self {
            servers: Arc::new(RwLock::new(HashMap::new())),
            tool_index: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn start_server(&self, config: McpServerConfig) -> AgentResult<ServerInfo> {
        if !config.enabled {
            return Err(AgentError::config("server is disabled"));
        }

        let mut cmd = Command::new(&config.command);
        cmd.args(&config.args);

        for (key, value) in &config.env {
            cmd.env(key, value);
        }

        if let Some(ref dir) = config.working_dir {
            cmd.current_dir(dir);
        }

        cmd.stdin(std::process::Stdio::piped());
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::null());

        let mut process = cmd
            .spawn()
            .map_err(|e| AgentError::io("spawn MCP server", e))?;

        let stdin = process
            .stdin
            .take()
            .ok_or_else(|| AgentError::mcp("failed to get stdin"))?;
        let stdout = process
            .stdout
            .take()
            .ok_or_else(|| AgentError::mcp("failed to get stdout"))?;

        let transport = StdioTransport::new(stdin, stdout);
        let mut client = McpClient::new(transport);

        let info = client.initialize("agent", "1.0.0").await?;
        let tools = client.list_tools().await.unwrap_or_default();
        let resources = client.list_resources().await.unwrap_or_default();

        let server_name = config.name.clone();

        {
            let mut index = self.tool_index.write().await;
            for tool in &tools {
                index.insert(tool.name.clone(), server_name.clone());
            }
        }

        let instance = McpServerInstance {
            config,
            process,
            client,
            info: info.clone(),
            tools,
            resources,
        };

        self.servers.write().await.insert(server_name, instance);

        Ok(info)
    }

    pub async fn stop_server(&self, name: &str) -> AgentResult<()> {
        let mut servers = self.servers.write().await;

        if let Some(mut instance) = servers.remove(name) {
            let _ = instance.client.shutdown().await;
            let _ = instance.process.kill().await;

            let mut index = self.tool_index.write().await;
            for tool in &instance.tools {
                index.remove(&tool.name);
            }
        }

        Ok(())
    }

    pub async fn stop_all(&self) -> AgentResult<()> {
        let names: Vec<String> = self.servers.read().await.keys().cloned().collect();

        for name in names {
            self.stop_server(&name).await?;
        }

        Ok(())
    }

    pub async fn list_servers(&self) -> Vec<String> {
        self.servers.read().await.keys().cloned().collect()
    }

    pub async fn get_server_info(&self, name: &str) -> Option<ServerInfo> {
        self.servers
            .read()
            .await
            .get(name)
            .map(|s| s.info.clone())
    }

    pub async fn list_all_tools(&self) -> Vec<(String, Tool)> {
        let servers = self.servers.read().await;
        let mut all_tools = Vec::new();

        for (name, instance) in servers.iter() {
            for tool in &instance.tools {
                all_tools.push((name.clone(), tool.clone()));
            }
        }

        all_tools
    }

    pub async fn list_all_resources(&self) -> Vec<(String, Resource)> {
        let servers = self.servers.read().await;
        let mut all_resources = Vec::new();

        for (name, instance) in servers.iter() {
            for resource in &instance.resources {
                all_resources.push((name.clone(), resource.clone()));
            }
        }

        all_resources
    }

    pub async fn find_tool(&self, tool_name: &str) -> Option<(String, Tool)> {
        let index = self.tool_index.read().await;

        if let Some(server_name) = index.get(tool_name) {
            let servers = self.servers.read().await;
            if let Some(instance) = servers.get(server_name) {
                if let Some(tool) = instance.tools.iter().find(|t| t.name == tool_name) {
                    return Some((server_name.clone(), tool.clone()));
                }
            }
        }

        None
    }

    pub async fn call_tool(
        &self,
        tool_name: &str,
        arguments: serde_json::Value,
    ) -> AgentResult<ToolCallResponse> {
        let server_name = {
            let index = self.tool_index.read().await;
            index
                .get(tool_name)
                .cloned()
                .ok_or_else(|| AgentError::not_found(format!("tool: {}", tool_name)))?
        };

        let mut servers = self.servers.write().await;
        let instance = servers
            .get_mut(&server_name)
            .ok_or_else(|| AgentError::not_found(format!("server: {}", server_name)))?;

        let request = ToolCallRequest {
            name: tool_name.to_string(),
            arguments,
        };

        instance.client.call_tool(request).await
    }

    pub async fn read_resource(&self, server_name: &str, uri: &str) -> AgentResult<String> {
        let mut servers = self.servers.write().await;
        let instance = servers
            .get_mut(server_name)
            .ok_or_else(|| AgentError::not_found(format!("server: {}", server_name)))?;

        let contents = instance.client.read_resource(uri).await?;

        let text = contents
            .into_iter()
            .filter_map(|c| c.text)
            .collect::<Vec<_>>()
            .join("\n");

        Ok(text)
    }

    pub async fn is_auto_approved(&self, server_name: &str, tool_name: &str) -> bool {
        let servers = self.servers.read().await;

        if let Some(instance) = servers.get(server_name) {
            return instance.config.auto_approve.contains(&tool_name.to_string())
                || instance.config.auto_approve.contains(&"*".to_string());
        }

        false
    }

    pub async fn refresh_tools(&self, server_name: &str) -> AgentResult<Vec<Tool>> {
        let mut servers = self.servers.write().await;
        let instance = servers
            .get_mut(server_name)
            .ok_or_else(|| AgentError::not_found(format!("server: {}", server_name)))?;

        let tools = instance.client.list_tools().await?;

        {
            let mut index = self.tool_index.write().await;
            for tool in &instance.tools {
                index.remove(&tool.name);
            }
            for tool in &tools {
                index.insert(tool.name.clone(), server_name.to_string());
            }
        }

        instance.tools = tools.clone();
        Ok(tools)
    }
}

impl Default for McpServerManager {
    fn default() -> Self {
        Self::new()
    }
}

pub fn format_tool_result(response: &ToolCallResponse) -> String {
    let mut output = String::new();

    for content in &response.content {
        match content {
            ToolContent::Text { text } => {
                output.push_str(text);
                output.push('\n');
            }
            ToolContent::Image { mime_type, .. } => {
                output.push_str(&format!("[Image: {}]\n", mime_type));
            }
            ToolContent::Resource { resource } => {
                if let Some(ref text) = resource.text {
                    output.push_str(text);
                    output.push('\n');
                } else {
                    output.push_str(&format!("[Resource: {}]\n", resource.uri));
                }
            }
        }
    }

    output
}
