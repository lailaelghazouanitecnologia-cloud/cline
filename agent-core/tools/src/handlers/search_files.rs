#![deny(clippy::all)]
#![forbid(unsafe_code)]

use crate::context::ToolContext;
use crate::handler::{ToolFuture, ToolHandler};
use crate::spec::{ToolCall, ToolOutput, ToolSpec};
use agent_common::AgentResult;
use std::path::Path;
use std::process::Stdio;
use tokio::process::Command;

pub struct SearchFilesHandler;

impl SearchFilesHandler {
    pub fn new() -> Self {
        Self
    }

    async fn search_impl(
        context: &ToolContext,
        pattern: String,
        path: Option<String>,
        file_pattern: Option<String>,
        case_sensitive: bool,
        max_results: usize,
    ) -> AgentResult<ToolOutput> {
        let search_path = path
            .map(|p| context.resolve_path(&p))
            .unwrap_or_else(|| context.working_directory().to_path_buf());

        let output = if is_ripgrep_available().await {
            search_with_ripgrep(
                &pattern,
                &search_path,
                file_pattern.as_deref(),
                case_sensitive,
                max_results,
            )
            .await?
        } else {
            search_with_grep(
                &pattern,
                &search_path,
                file_pattern.as_deref(),
                case_sensitive,
                max_results,
            )
            .await?
        };

        Ok(ToolOutput::success(output))
    }
}

impl Default for SearchFilesHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolHandler for SearchFilesHandler {
    fn spec(&self) -> ToolSpec {
        ToolSpec::new("search_files", "Search for pattern in files using ripgrep or grep")
            .with_parameter("pattern", "string", "Regex pattern to search", true)
            .with_parameter("path", "string", "Directory to search in", false)
            .with_parameter("file_pattern", "string", "Glob pattern for files (e.g. *.rs)", false)
            .with_parameter("case_sensitive", "boolean", "Case sensitive search", false)
            .with_parameter("max_results", "integer", "Maximum results to return", false)
    }

    fn execute(&self, context: &ToolContext, call: ToolCall) -> ToolFuture {
        let pattern = call.get_string("pattern").unwrap_or_default();
        let path = call.get_string("path");
        let file_pattern = call.get_string("file_pattern");
        let case_sensitive = call.get_bool("case_sensitive").unwrap_or(false);
        let max_results = call.get_u64("max_results").unwrap_or(100) as usize;
        let context = context.clone();

        Box::pin(async move {
            Self::search_impl(&context, pattern, path, file_pattern, case_sensitive, max_results)
                .await
        })
    }

    fn is_dangerous(&self, _call: &ToolCall) -> bool {
        false
    }
}

async fn is_ripgrep_available() -> bool {
    Command::new("rg")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await
        .map(|s| s.success())
        .unwrap_or(false)
}

async fn search_with_ripgrep(
    pattern: &str,
    path: &Path,
    file_pattern: Option<&str>,
    case_sensitive: bool,
    max_results: usize,
) -> AgentResult<String> {
    let mut cmd = Command::new("rg");
    cmd.arg("--line-number")
        .arg("--with-filename")
        .arg("--max-count")
        .arg(max_results.to_string());

    if !case_sensitive {
        cmd.arg("--ignore-case");
    }

    if let Some(glob) = file_pattern {
        cmd.arg("--glob").arg(glob);
    }

    cmd.arg(pattern).arg(path);

    let output = cmd
        .output()
        .await
        .map_err(|e| agent_common::AgentError::tool_execution("search_files", e.to_string()))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if !output.status.success() && stdout.is_empty() {
        if stderr.contains("No such file or directory") {
            return Ok("No matches found".to_string());
        }
        return Ok(format!("Search completed with no matches\n{}", stderr));
    }

    let matches: Vec<&str> = stdout.lines().take(max_results).collect();
    let result = format!(
        "Found {} matches:\n\n{}",
        matches.len(),
        matches.join("\n")
    );

    Ok(result)
}

async fn search_with_grep(
    pattern: &str,
    path: &Path,
    file_pattern: Option<&str>,
    case_sensitive: bool,
    max_results: usize,
) -> AgentResult<String> {
    let mut cmd = Command::new("grep");
    cmd.arg("-r")
        .arg("-n")
        .arg("--include")
        .arg(file_pattern.unwrap_or("*"));

    if !case_sensitive {
        cmd.arg("-i");
    }

    cmd.arg(pattern).arg(path);

    let output = cmd
        .output()
        .await
        .map_err(|e| agent_common::AgentError::tool_execution("search_files", e.to_string()))?;

    let stdout = String::from_utf8_lossy(&output.stdout);

    if stdout.is_empty() {
        return Ok("No matches found".to_string());
    }

    let matches: Vec<&str> = stdout.lines().take(max_results).collect();
    let result = format!(
        "Found {} matches:\n\n{}",
        matches.len(),
        matches.join("\n")
    );

    Ok(result)
}
