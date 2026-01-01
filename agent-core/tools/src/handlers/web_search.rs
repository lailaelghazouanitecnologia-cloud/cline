#![deny(clippy::all)]
#![forbid(unsafe_code)]

use crate::handler::{ToolFuture, ToolHandler};
use crate::spec::ToolSpec;
use crate::{ToolCall, ToolContext, ToolOutput};
use agent_common::{AgentError, AgentResult};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub title: String,
    pub url: String,
    pub snippet: String,
    pub position: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResponse {
    pub query: String,
    pub results: Vec<SearchResult>,
    pub total_results: Option<u64>,
}

pub trait SearchProvider: Send + Sync {
    fn search(
        &self,
        query: &str,
        options: &SearchOptions,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = AgentResult<SearchResponse>> + Send>>;
}

#[derive(Debug, Clone, Default)]
pub struct SearchOptions {
    pub max_results: usize,
    pub allowed_domains: Vec<String>,
    pub blocked_domains: Vec<String>,
    pub language: Option<String>,
    pub region: Option<String>,
    pub safe_search: bool,
}

impl SearchOptions {
    pub fn new() -> Self {
        Self {
            max_results: 10,
            allowed_domains: Vec::new(),
            blocked_domains: Vec::new(),
            language: None,
            region: None,
            safe_search: true,
        }
    }

    pub fn with_max_results(mut self, max: usize) -> Self {
        self.max_results = max;
        self
    }

    pub fn allow_domain(mut self, domain: impl Into<String>) -> Self {
        self.allowed_domains.push(domain.into());
        self
    }

    pub fn block_domain(mut self, domain: impl Into<String>) -> Self {
        self.blocked_domains.push(domain.into());
        self
    }
}

pub struct DuckDuckGoProvider {
    client: reqwest::Client,
}

impl DuckDuckGoProvider {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .user_agent("Mozilla/5.0 (compatible; AgentBot/1.0)")
                .build()
                .unwrap_or_default(),
        }
    }
}

impl Default for DuckDuckGoProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl SearchProvider for DuckDuckGoProvider {
    fn search(
        &self,
        query: &str,
        options: &SearchOptions,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = AgentResult<SearchResponse>> + Send>>
    {
        let client = self.client.clone();
        let query = query.to_string();
        let max_results = options.max_results;
        let blocked = options.blocked_domains.clone();

        Box::pin(async move {
            let url = format!(
                "https://html.duckduckgo.com/html/?q={}",
                urlencoding::encode(&query)
            );

            let response = client
                .get(&url)
                .send()
                .await
                .map_err(|e| AgentError::api(format!("search request failed: {}", e)))?;

            let html = response
                .text()
                .await
                .map_err(|e| AgentError::api(format!("failed to read response: {}", e)))?;

            let results = parse_duckduckgo_html(&html, max_results, &blocked);

            Ok(SearchResponse {
                query,
                results,
                total_results: None,
            })
        })
    }
}

fn parse_duckduckgo_html(
    html: &str,
    max_results: usize,
    blocked_domains: &[String],
) -> Vec<SearchResult> {
    let mut results = Vec::new();
    let mut position = 1;

    for line in html.lines() {
        if results.len() >= max_results {
            break;
        }

        if line.contains("class=\"result__a\"") {
            if let Some(result) = parse_result_line(line, position) {
                let dominated_blocked = blocked_domains
                    .iter()
                    .any(|d| result.url.contains(d));

                if !dominated_blocked {
                    results.push(result);
                    position += 1;
                }
            }
        }
    }

    results
}

fn parse_result_line(line: &str, position: usize) -> Option<SearchResult> {
    let href_start = line.find("href=\"")? + 6;
    let href_end = line[href_start..].find('"')? + href_start;
    let url = &line[href_start..href_end];

    let url = if url.starts_with("//duckduckgo.com/l/?uddg=") {
        let encoded = url.trim_start_matches("//duckduckgo.com/l/?uddg=");
        let decoded = urlencoding::decode(encoded).ok()?;
        let end = decoded.find('&').unwrap_or(decoded.len());
        decoded[..end].to_string()
    } else {
        url.to_string()
    };

    let title_start = line.find('>')? + 1;
    let title_end = line[title_start..].find('<').unwrap_or(0) + title_start;
    let title = html_decode(&line[title_start..title_end]);

    Some(SearchResult {
        title,
        url,
        snippet: String::new(),
        position,
    })
}

fn html_decode(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
}

pub struct WebSearchHandler {
    provider: Box<dyn SearchProvider>,
    default_options: SearchOptions,
}

impl WebSearchHandler {
    pub fn new() -> Self {
        Self {
            provider: Box::new(DuckDuckGoProvider::new()),
            default_options: SearchOptions::new(),
        }
    }

    pub fn with_provider(mut self, provider: impl SearchProvider + 'static) -> Self {
        self.provider = Box::new(provider);
        self
    }

    pub fn with_options(mut self, options: SearchOptions) -> Self {
        self.default_options = options;
        self
    }

    fn format_results(&self, response: &SearchResponse) -> String {
        let mut output = format!("Search results for: {}\n\n", response.query);

        if response.results.is_empty() {
            output.push_str("No results found.");
            return output;
        }

        for result in &response.results {
            output.push_str(&format!(
                "{}. {}\n   {}\n",
                result.position, result.title, result.url
            ));

            if !result.snippet.is_empty() {
                output.push_str(&format!("   {}\n", result.snippet));
            }

            output.push('\n');
        }

        output
    }
}

impl Default for WebSearchHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolHandler for WebSearchHandler {
    fn spec(&self) -> ToolSpec {
        ToolSpec::new("web_search", "Search the web for information")
            .with_parameter("query", "string", "The search query", true)
            .with_parameter("max_results", "integer", "Maximum results (default: 10)", false)
            .with_parameter("allowed_domains", "array", "Only return results from these domains", false)
            .with_parameter("blocked_domains", "array", "Exclude results from these domains", false)
    }

    fn execute(&self, _ctx: &ToolContext, call: ToolCall) -> ToolFuture {
        let args = call.to_json_value();
        let mut options = self.default_options.clone();

        let query = match args.get("query").and_then(|v| v.as_str()) {
            Some(q) => q.to_string(),
            None => {
                return Box::pin(async {
                    Err(AgentError::validation("query is required"))
                });
            }
        };

        if let Some(max) = args.get("max_results").and_then(|v| v.as_u64()) {
            options.max_results = max as usize;
        }

        if let Some(domains) = args.get("allowed_domains").and_then(|v| v.as_array()) {
            for d in domains {
                if let Some(domain) = d.as_str() {
                    options.allowed_domains.push(domain.to_string());
                }
            }
        }

        if let Some(domains) = args.get("blocked_domains").and_then(|v| v.as_array()) {
            for d in domains {
                if let Some(domain) = d.as_str() {
                    options.blocked_domains.push(domain.to_string());
                }
            }
        }

        let provider = DuckDuckGoProvider::new();

        Box::pin(async move {
            let response = provider.search(&query, &options).await?;

            let handler = WebSearchHandler::new();
            let output = handler.format_results(&response);

            Ok(ToolOutput::success(output))
        })
    }

    fn is_dangerous(&self, _call: &ToolCall) -> bool {
        false
    }
}
