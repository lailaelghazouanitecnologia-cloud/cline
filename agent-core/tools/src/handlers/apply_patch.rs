#![deny(clippy::all)]
#![forbid(unsafe_code)]

use crate::context::ToolContext;
use crate::handler::{ToolFuture, ToolHandler};
use crate::spec::{ToolCall, ToolOutput, ToolSpec};
use agent_common::AgentResult;
use tokio::fs;

pub struct ApplyPatchHandler;

impl ApplyPatchHandler {
    pub fn new() -> Self {
        Self
    }

    async fn apply_patch_impl(
        context: &ToolContext,
        path: String,
        patch: String,
    ) -> AgentResult<ToolOutput> {
        let full_path = context.resolve_path(&path);

        let original = fs::read_to_string(&full_path)
            .await
            .unwrap_or_default();

        let patched = apply_unified_diff(&original, &patch)?;

        fs::write(&full_path, &patched)
            .await
            .map_err(|e| agent_common::AgentError::tool_execution("apply_patch", e.to_string()))?;

        let output = format!(
            "Successfully applied patch to {}\nChanges: {} lines modified",
            path,
            count_changes(&patch)
        );

        Ok(ToolOutput::success(output))
    }
}

impl Default for ApplyPatchHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolHandler for ApplyPatchHandler {
    fn spec(&self) -> ToolSpec {
        ToolSpec::new("apply_patch", "Apply a unified diff patch to a file")
            .with_parameter("path", "string", "File path to patch", true)
            .with_parameter("patch", "string", "Unified diff content", true)
    }

    fn execute(&self, context: &ToolContext, call: ToolCall) -> ToolFuture {
        let path = call.get_string("path").unwrap_or_default();
        let patch = call.get_string("patch").unwrap_or_default();
        let context = context.clone();

        Box::pin(async move {
            Self::apply_patch_impl(&context, path, patch).await
        })
    }

    fn is_dangerous(&self, _call: &ToolCall) -> bool {
        true
    }
}

fn apply_unified_diff(original: &str, patch: &str) -> AgentResult<String> {
    let original_lines: Vec<&str> = original.lines().collect();
    let mut result_lines: Vec<String> = original_lines.iter().map(|s| s.to_string()).collect();

    let mut current_line: i64 = 0;
    let mut offset: i64 = 0;

    for line in patch.lines() {
        if line.starts_with("@@") {
            if let Some((start, _)) = parse_hunk_header(line) {
                current_line = (start as i64) - 1 + offset;
            }
            continue;
        }

        if line.starts_with("---") || line.starts_with("+++") {
            continue;
        }

        if line.starts_with('-') {
            let idx = current_line as usize;
            if idx < result_lines.len() {
                result_lines.remove(idx);
                offset -= 1;
            }
            continue;
        }

        if line.starts_with('+') {
            let content = &line[1..];
            let idx = current_line as usize;
            if idx <= result_lines.len() {
                result_lines.insert(idx, content.to_string());
                offset += 1;
                current_line += 1;
            }
            continue;
        }

        if line.starts_with(' ') || !line.is_empty() {
            current_line += 1;
        }
    }

    Ok(result_lines.join("\n"))
}

fn parse_hunk_header(header: &str) -> Option<(usize, usize)> {
    let parts: Vec<&str> = header.split_whitespace().collect();
    if parts.len() < 2 {
        return None;
    }

    let old_range = parts.get(1)?;
    let old_range = old_range.trim_start_matches('-');
    let old_parts: Vec<&str> = old_range.split(',').collect();
    let start: usize = old_parts.first()?.parse().ok()?;
    let count: usize = old_parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(1);

    Some((start, count))
}

fn count_changes(patch: &str) -> usize {
    patch
        .lines()
        .filter(|line| line.starts_with('+') || line.starts_with('-'))
        .filter(|line| !line.starts_with("+++") && !line.starts_with("---"))
        .count()
}
