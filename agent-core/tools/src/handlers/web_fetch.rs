#![deny(clippy::all)]
#![forbid(unsafe_code)]

use crate::context::ToolContext;
use crate::handler::{ToolFuture, ToolHandler};
use crate::spec::{ToolCall, ToolOutput, ToolSpec};
use agent_common::{AgentError, AgentResult};
use std::time::Duration;

pub struct WebFetchHandler {
    client: reqwest::Client,
}

impl WebFetchHandler {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .user_agent("Mozilla/5.0 (compatible; AgentCore/1.0)")
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        Self { client }
    }

    async fn fetch_impl(
        &self,
        url: String,
        method: String,
        headers: Option<serde_json::Value>,
        body: Option<String>,
    ) -> AgentResult<ToolOutput> {
        let method = method.to_uppercase();
        let request = match method.as_str() {
            "GET" => self.client.get(&url),
            "POST" => self.client.post(&url),
            "PUT" => self.client.put(&url),
            "DELETE" => self.client.delete(&url),
            "HEAD" => self.client.head(&url),
            "PATCH" => self.client.patch(&url),
            _ => {
                return Err(AgentError::tool_execution(
                    "web_fetch",
                    format!("unsupported method: {}", method),
                ))
            }
        };

        let mut request = request;

        if let Some(headers_obj) = headers {
            if let Some(obj) = headers_obj.as_object() {
                for (key, value) in obj {
                    if let Some(val) = value.as_str() {
                        request = request.header(key.as_str(), val);
                    }
                }
            }
        }

        if let Some(body_content) = body {
            request = request.body(body_content);
        }

        let response = request
            .send()
            .await
            .map_err(|e| AgentError::tool_execution("web_fetch", e.to_string()))?;

        let status = response.status();
        let headers = format_headers(response.headers());
        let body = response
            .text()
            .await
            .map_err(|e| AgentError::tool_execution("web_fetch", e.to_string()))?;

        let truncated_body = if body.len() > 50000 {
            format!("{}...\n\n[Truncated: {} total bytes]", &body[..50000], body.len())
        } else {
            body
        };

        let output = format!(
            "Status: {}\n\nHeaders:\n{}\n\nBody:\n{}",
            status, headers, truncated_body
        );

        Ok(ToolOutput::success(output))
    }
}

impl Default for WebFetchHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolHandler for WebFetchHandler {
    fn spec(&self) -> ToolSpec {
        ToolSpec::new("web_fetch", "Fetch content from a URL")
            .with_parameter("url", "string", "URL to fetch", true)
            .with_parameter("method", "string", "HTTP method (GET, POST, etc.)", false)
            .with_parameter("headers", "object", "Request headers", false)
            .with_parameter("body", "string", "Request body", false)
    }

    fn execute(&self, _context: &ToolContext, call: ToolCall) -> ToolFuture {
        let url = call.get_string("url").unwrap_or_default();
        let method = call.get_string("method").unwrap_or_else(|| "GET".to_string());
        let headers = call.get_value("headers");
        let body = call.get_string("body");

        let client = self.client.clone();
        let handler = Self { client };

        Box::pin(async move { handler.fetch_impl(url, method, headers, body).await })
    }

    fn is_dangerous(&self, _call: &ToolCall) -> bool {
        false
    }
}

fn format_headers(headers: &reqwest::header::HeaderMap) -> String {
    headers
        .iter()
        .map(|(name, value)| {
            format!(
                "{}: {}",
                name.as_str(),
                value.to_str().unwrap_or("<binary>")
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}
