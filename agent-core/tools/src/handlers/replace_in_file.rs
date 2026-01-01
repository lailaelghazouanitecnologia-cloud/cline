#![deny(clippy::all)]
#![forbid(unsafe_code)]

use crate::context::ToolContext;
use crate::handler::{ToolFuture, ToolHandler};
use crate::spec::{ToolCall, ToolOutput, ToolSpec};
use agent_common::AgentResult;
use tokio::fs;

pub struct ReplaceInFileHandler;

impl ReplaceInFileHandler {
    fn spec() -> ToolSpec {
        ToolSpec::new(
            "replace_in_file",
            "Replace specific content in a file with new content. Use for surgical edits.",
        )
        .with_parameter("path", "string", "The file path to modify", true)
        .with_parameter("old_str", "string", "The exact text to find and replace", true)
        .with_parameter("new_str", "string", "The text to replace with", true)
        .with_parameter("count", "integer", "Max replacements (0 = all, default 1)", false)
    }

    async fn execute_impl(
        context: ToolContext,
        path: String,
        old_str: String,
        new_str: String,
        count: Option<i64>,
    ) -> AgentResult<ToolOutput> {
        let full_path = context.resolve_path(&path);

        if !full_path.exists() {
            return Ok(ToolOutput::failure(format!(
                "File not found: {}",
                full_path.display()
            )));
        }

        let original = fs::read_to_string(&full_path)
            .await
            .map_err(|e| agent_common::AgentError::io("read file", e))?;

        if !original.contains(&old_str) {
            return Ok(ToolOutput::failure(format!(
                "The specified text was not found in {}:\n---\n{}\n---",
                path,
                truncate(&old_str, 200)
            )));
        }

        let max_replacements = count.unwrap_or(1);
        let (modified, replacement_count) = if max_replacements <= 0 {
            let count = original.matches(&old_str).count();
            (original.replace(&old_str, &new_str), count)
        } else {
            replace_n(&original, &old_str, &new_str, max_replacements as usize)
        };

        if replacement_count == 0 {
            return Ok(ToolOutput::failure("No replacements were made."));
        }

        fs::write(&full_path, &modified)
            .await
            .map_err(|e| agent_common::AgentError::io("write file", e))?;

        let message = if replacement_count == 1 {
            format!("Replaced 1 occurrence in {}", path)
        } else {
            format!("Replaced {} occurrences in {}", replacement_count, path)
        };

        Ok(ToolOutput::success(message))
    }
}

fn replace_n(text: &str, old: &str, new: &str, max: usize) -> (String, usize) {
    let mut result = String::with_capacity(text.len());
    let mut count = 0;
    let mut last_end = 0;

    for (start, part) in text.match_indices(old) {
        if count >= max {
            break;
        }
        result.push_str(&text[last_end..start]);
        result.push_str(new);
        last_end = start + part.len();
        count += 1;
    }

    result.push_str(&text[last_end..]);
    (result, count)
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len])
    }
}

impl ToolHandler for ReplaceInFileHandler {
    fn spec(&self) -> ToolSpec {
        Self::spec()
    }

    fn execute(&self, context: &ToolContext, call: ToolCall) -> ToolFuture {
        let ctx = context.clone();
        let path = call.get_string("path").unwrap_or_default();
        let old_str = call.get_string("old_str").unwrap_or_default();
        let new_str = call.get_string("new_str").unwrap_or_default();
        let count = call.get_i64("count");

        Box::pin(async move {
            if path.is_empty() {
                return Ok(ToolOutput::failure("Missing required parameter: path"));
            }
            if old_str.is_empty() {
                return Ok(ToolOutput::failure("Missing required parameter: old_str"));
            }
            Self::execute_impl(ctx, path, old_str, new_str, count).await
        })
    }

    fn is_dangerous(&self, _call: &ToolCall) -> bool {
        true
    }
}

pub struct InsertCodeBlockHandler;

impl InsertCodeBlockHandler {
    fn spec() -> ToolSpec {
        ToolSpec::new(
            "insert_code_block",
            "Insert a code block at a specific location in a file.",
        )
        .with_parameter("path", "string", "The file path to modify", true)
        .with_parameter("position", "string", "Where to insert: 'after' or 'before'", true)
        .with_parameter("anchor", "string", "The text to anchor the insertion to", true)
        .with_parameter("content", "string", "The code block to insert", true)
    }

    async fn execute_impl(
        context: ToolContext,
        path: String,
        position: String,
        anchor: String,
        content: String,
    ) -> AgentResult<ToolOutput> {
        let full_path = context.resolve_path(&path);

        if !full_path.exists() {
            return Ok(ToolOutput::failure(format!(
                "File not found: {}",
                full_path.display()
            )));
        }

        let original = fs::read_to_string(&full_path)
            .await
            .map_err(|e| agent_common::AgentError::io("read file", e))?;

        let anchor_pos = original.find(&anchor);
        if anchor_pos.is_none() {
            return Ok(ToolOutput::failure(format!(
                "Anchor text not found in {}:\n---\n{}\n---",
                path,
                truncate(&anchor, 200)
            )));
        }

        let anchor_pos = anchor_pos.unwrap();
        let modified = match position.to_lowercase().as_str() {
            "before" => {
                let mut result = String::with_capacity(original.len() + content.len() + 1);
                result.push_str(&original[..anchor_pos]);
                result.push_str(&content);
                result.push('\n');
                result.push_str(&original[anchor_pos..]);
                result
            }
            "after" => {
                let end_pos = anchor_pos + anchor.len();
                let mut result = String::with_capacity(original.len() + content.len() + 1);
                result.push_str(&original[..end_pos]);
                result.push('\n');
                result.push_str(&content);
                result.push_str(&original[end_pos..]);
                result
            }
            _ => {
                return Ok(ToolOutput::failure(
                    "Position must be 'before' or 'after'",
                ));
            }
        };

        fs::write(&full_path, &modified)
            .await
            .map_err(|e| agent_common::AgentError::io("write file", e))?;

        Ok(ToolOutput::success(format!(
            "Inserted code block {} anchor in {}",
            position, path
        )))
    }
}

impl ToolHandler for InsertCodeBlockHandler {
    fn spec(&self) -> ToolSpec {
        Self::spec()
    }

    fn execute(&self, context: &ToolContext, call: ToolCall) -> ToolFuture {
        let ctx = context.clone();
        let path = call.get_string("path").unwrap_or_default();
        let position = call.get_string("position").unwrap_or_default();
        let anchor = call.get_string("anchor").unwrap_or_default();
        let content = call.get_string("content").unwrap_or_default();

        Box::pin(async move {
            if path.is_empty() {
                return Ok(ToolOutput::failure("Missing required parameter: path"));
            }
            if anchor.is_empty() {
                return Ok(ToolOutput::failure("Missing required parameter: anchor"));
            }
            if content.is_empty() {
                return Ok(ToolOutput::failure("Missing required parameter: content"));
            }
            Self::execute_impl(ctx, path, position, anchor, content).await
        })
    }

    fn is_dangerous(&self, _call: &ToolCall) -> bool {
        true
    }
}
