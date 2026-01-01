#![deny(clippy::all)]
#![forbid(unsafe_code)]

use crate::context::ToolContext;
use crate::handler::{ToolFuture, ToolHandler};
use crate::spec::{ToolCall, ToolOutput, ToolSpec};
use agent_common::AgentResult;
use std::path::Path;
use tokio::fs;

pub struct ListFilesHandler;

impl ListFilesHandler {
    pub fn new() -> Self {
        Self
    }

    async fn list_impl(
        context: &ToolContext,
        path: Option<String>,
        recursive: bool,
        max_depth: usize,
        show_hidden: bool,
    ) -> AgentResult<ToolOutput> {
        let target_path = path
            .map(|p| context.resolve_path(&p))
            .unwrap_or_else(|| context.working_directory().to_path_buf());

        let entries = if recursive {
            list_recursive(&target_path, 0, max_depth, show_hidden).await?
        } else {
            list_directory(&target_path, show_hidden).await?
        };

        let output = if entries.is_empty() {
            "Directory is empty".to_string()
        } else {
            entries.join("\n")
        };

        Ok(ToolOutput::success(output))
    }
}

impl Default for ListFilesHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolHandler for ListFilesHandler {
    fn spec(&self) -> ToolSpec {
        ToolSpec::new("list_files", "List files and directories")
            .with_parameter("path", "string", "Directory path to list", false)
            .with_parameter("recursive", "boolean", "List recursively", false)
            .with_parameter("max_depth", "integer", "Maximum recursion depth", false)
            .with_parameter("show_hidden", "boolean", "Show hidden files", false)
    }

    fn execute(&self, context: &ToolContext, call: ToolCall) -> ToolFuture {
        let path = call.get_string("path");
        let recursive = call.get_bool("recursive").unwrap_or(false);
        let max_depth = call.get_u64("max_depth").unwrap_or(3) as usize;
        let show_hidden = call.get_bool("show_hidden").unwrap_or(false);
        let context = context.clone();

        Box::pin(async move {
            Self::list_impl(&context, path, recursive, max_depth, show_hidden).await
        })
    }

    fn is_dangerous(&self, _call: &ToolCall) -> bool {
        false
    }
}

async fn list_directory(path: &Path, show_hidden: bool) -> AgentResult<Vec<String>> {
    let mut entries = Vec::new();
    let mut dir = fs::read_dir(path)
        .await
        .map_err(|e| agent_common::AgentError::tool_execution("list_files", e.to_string()))?;

    while let Some(entry) = dir
        .next_entry()
        .await
        .map_err(|e| agent_common::AgentError::tool_execution("list_files", e.to_string()))?
    {
        let name = entry.file_name().to_string_lossy().to_string();

        if !show_hidden && name.starts_with('.') {
            continue;
        }

        let metadata = entry.metadata().await.ok();
        let file_type = if metadata.as_ref().map(|m| m.is_dir()).unwrap_or(false) {
            "📁"
        } else {
            "📄"
        };

        let size = metadata
            .as_ref()
            .map(|m| format_size(m.len()))
            .unwrap_or_default();

        entries.push(format!("{} {} {}", file_type, name, size));
    }

    entries.sort();
    Ok(entries)
}

async fn list_recursive(
    path: &Path,
    current_depth: usize,
    max_depth: usize,
    show_hidden: bool,
) -> AgentResult<Vec<String>> {
    if current_depth >= max_depth {
        return Ok(Vec::new());
    }

    let mut entries = Vec::new();
    let indent = "  ".repeat(current_depth);

    let mut dir = match fs::read_dir(path).await {
        Ok(dir) => dir,
        Err(_) => return Ok(Vec::new()),
    };

    while let Ok(Some(entry)) = dir.next_entry().await {
        let name = entry.file_name().to_string_lossy().to_string();

        if !show_hidden && name.starts_with('.') {
            continue;
        }

        let metadata = entry.metadata().await.ok();
        let is_dir = metadata.as_ref().map(|m| m.is_dir()).unwrap_or(false);

        if is_dir {
            entries.push(format!("{}📁 {}/", indent, name));
            let sub_entries = Box::pin(list_recursive(
                &entry.path(),
                current_depth + 1,
                max_depth,
                show_hidden,
            ))
            .await?;
            entries.extend(sub_entries);
        } else {
            let size = metadata
                .as_ref()
                .map(|m| format_size(m.len()))
                .unwrap_or_default();
            entries.push(format!("{}📄 {} {}", indent, name, size));
        }
    }

    Ok(entries)
}

fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{}B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1}KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1}MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.1}GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}
