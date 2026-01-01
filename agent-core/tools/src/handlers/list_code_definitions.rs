#![deny(clippy::all)]
#![forbid(unsafe_code)]

use crate::context::ToolContext;
use crate::handler::{ToolFuture, ToolHandler};
use crate::spec::{ToolCall, ToolOutput, ToolSpec};
use agent_common::AgentResult;
use tokio::fs;

pub struct ListCodeDefinitionsHandler;

impl ListCodeDefinitionsHandler {
    pub fn new() -> Self {
        Self
    }

    async fn analyze_impl(context: &ToolContext, path: String) -> AgentResult<ToolOutput> {
        let full_path = context.resolve_path(&path);

        let content = fs::read_to_string(&full_path)
            .await
            .map_err(|e| agent_common::AgentError::tool_execution("list_code_definitions", e.to_string()))?;

        let extension = full_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");

        let definitions = extract_definitions(&content, extension);

        if definitions.is_empty() {
            return Ok(ToolOutput::success("No definitions found in file".to_string()));
        }

        let output = format!(
            "Definitions in {}:\n\n{}",
            path,
            definitions.join("\n")
        );

        Ok(ToolOutput::success(output))
    }
}

impl Default for ListCodeDefinitionsHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolHandler for ListCodeDefinitionsHandler {
    fn spec(&self) -> ToolSpec {
        ToolSpec::new(
            "list_code_definitions",
            "List function, class, and type definitions in a source file",
        )
        .with_parameter("path", "string", "Path to source file", true)
    }

    fn execute(&self, context: &ToolContext, call: ToolCall) -> ToolFuture {
        let path = call.get_string("path").unwrap_or_default();
        let context = context.clone();

        Box::pin(async move { Self::analyze_impl(&context, path).await })
    }

    fn is_dangerous(&self, _call: &ToolCall) -> bool {
        false
    }
}

fn extract_definitions(content: &str, extension: &str) -> Vec<String> {
    let mut definitions = Vec::new();

    match extension {
        "rs" => extract_rust_definitions(content, &mut definitions),
        "ts" | "tsx" | "js" | "jsx" => extract_typescript_definitions(content, &mut definitions),
        "py" => extract_python_definitions(content, &mut definitions),
        "go" => extract_go_definitions(content, &mut definitions),
        "java" | "kt" => extract_java_definitions(content, &mut definitions),
        "c" | "cpp" | "h" | "hpp" => extract_c_definitions(content, &mut definitions),
        _ => extract_generic_definitions(content, &mut definitions),
    }

    definitions
}

fn extract_rust_definitions(content: &str, defs: &mut Vec<String>) {
    for (line_num, line) in content.lines().enumerate() {
        let trimmed = line.trim();

        if trimmed.starts_with("pub fn ")
            || trimmed.starts_with("fn ")
            || trimmed.starts_with("async fn ")
            || trimmed.starts_with("pub async fn ")
        {
            if let Some(name) = extract_fn_name(trimmed) {
                defs.push(format!("  L{}: fn {}", line_num + 1, name));
            }
        } else if trimmed.starts_with("pub struct ")
            || trimmed.starts_with("struct ")
        {
            if let Some(name) = extract_struct_name(trimmed) {
                defs.push(format!("  L{}: struct {}", line_num + 1, name));
            }
        } else if trimmed.starts_with("pub enum ") || trimmed.starts_with("enum ") {
            if let Some(name) = extract_enum_name(trimmed) {
                defs.push(format!("  L{}: enum {}", line_num + 1, name));
            }
        } else if trimmed.starts_with("pub trait ") || trimmed.starts_with("trait ") {
            if let Some(name) = extract_trait_name(trimmed) {
                defs.push(format!("  L{}: trait {}", line_num + 1, name));
            }
        } else if trimmed.starts_with("impl ") {
            if let Some(name) = extract_impl_name(trimmed) {
                defs.push(format!("  L{}: impl {}", line_num + 1, name));
            }
        }
    }
}

fn extract_typescript_definitions(content: &str, defs: &mut Vec<String>) {
    for (line_num, line) in content.lines().enumerate() {
        let trimmed = line.trim();

        if trimmed.starts_with("function ")
            || trimmed.starts_with("async function ")
            || trimmed.starts_with("export function ")
            || trimmed.starts_with("export async function ")
        {
            if let Some(name) = extract_ts_fn_name(trimmed) {
                defs.push(format!("  L{}: function {}", line_num + 1, name));
            }
        } else if trimmed.starts_with("class ")
            || trimmed.starts_with("export class ")
        {
            if let Some(name) = extract_class_name(trimmed) {
                defs.push(format!("  L{}: class {}", line_num + 1, name));
            }
        } else if trimmed.starts_with("interface ")
            || trimmed.starts_with("export interface ")
        {
            if let Some(name) = extract_interface_name(trimmed) {
                defs.push(format!("  L{}: interface {}", line_num + 1, name));
            }
        } else if trimmed.starts_with("type ")
            || trimmed.starts_with("export type ")
        {
            if let Some(name) = extract_type_name(trimmed) {
                defs.push(format!("  L{}: type {}", line_num + 1, name));
            }
        } else if trimmed.contains(" = (") || trimmed.contains(" = async (") {
            if let Some(name) = extract_arrow_fn_name(trimmed) {
                defs.push(format!("  L{}: const {} (arrow fn)", line_num + 1, name));
            }
        }
    }
}

fn extract_python_definitions(content: &str, defs: &mut Vec<String>) {
    for (line_num, line) in content.lines().enumerate() {
        let trimmed = line.trim();

        if trimmed.starts_with("def ") || trimmed.starts_with("async def ") {
            if let Some(name) = extract_py_fn_name(trimmed) {
                defs.push(format!("  L{}: def {}", line_num + 1, name));
            }
        } else if trimmed.starts_with("class ") {
            if let Some(name) = extract_py_class_name(trimmed) {
                defs.push(format!("  L{}: class {}", line_num + 1, name));
            }
        }
    }
}

fn extract_go_definitions(content: &str, defs: &mut Vec<String>) {
    for (line_num, line) in content.lines().enumerate() {
        let trimmed = line.trim();

        if trimmed.starts_with("func ") {
            if let Some(name) = extract_go_fn_name(trimmed) {
                defs.push(format!("  L{}: func {}", line_num + 1, name));
            }
        } else if trimmed.starts_with("type ") && trimmed.contains(" struct") {
            if let Some(name) = extract_go_type_name(trimmed) {
                defs.push(format!("  L{}: type {} struct", line_num + 1, name));
            }
        } else if trimmed.starts_with("type ") && trimmed.contains(" interface") {
            if let Some(name) = extract_go_type_name(trimmed) {
                defs.push(format!("  L{}: type {} interface", line_num + 1, name));
            }
        }
    }
}

fn extract_java_definitions(content: &str, defs: &mut Vec<String>) {
    for (line_num, line) in content.lines().enumerate() {
        let trimmed = line.trim();

        if (trimmed.contains("public ") || trimmed.contains("private ") || trimmed.contains("protected "))
            && trimmed.contains("class ")
        {
            if let Some(name) = extract_java_class_name(trimmed) {
                defs.push(format!("  L{}: class {}", line_num + 1, name));
            }
        } else if trimmed.contains("interface ") {
            if let Some(name) = extract_java_interface_name(trimmed) {
                defs.push(format!("  L{}: interface {}", line_num + 1, name));
            }
        }
    }
}

fn extract_c_definitions(content: &str, defs: &mut Vec<String>) {
    for (line_num, line) in content.lines().enumerate() {
        let trimmed = line.trim();

        if trimmed.contains("(") && !trimmed.starts_with("//") && !trimmed.starts_with("/*") {
            if let Some(name) = extract_c_fn_name(trimmed) {
                defs.push(format!("  L{}: {}", line_num + 1, name));
            }
        } else if trimmed.starts_with("struct ") || trimmed.starts_with("typedef struct") {
            if let Some(name) = extract_c_struct_name(trimmed) {
                defs.push(format!("  L{}: struct {}", line_num + 1, name));
            }
        }
    }
}

fn extract_generic_definitions(content: &str, defs: &mut Vec<String>) {
    for (line_num, line) in content.lines().enumerate() {
        let trimmed = line.trim();

        if trimmed.starts_with("function ")
            || trimmed.starts_with("def ")
            || trimmed.starts_with("fn ")
            || trimmed.starts_with("func ")
        {
            defs.push(format!("  L{}: {}", line_num + 1, trimmed.chars().take(60).collect::<String>()));
        }
    }
}

fn extract_fn_name(line: &str) -> Option<String> {
    let start = line.find("fn ")? + 3;
    let rest = &line[start..];
    let end = rest.find('(')?;
    Some(rest[..end].trim().to_string())
}

fn extract_struct_name(line: &str) -> Option<String> {
    let start = line.find("struct ")? + 7;
    let rest = &line[start..];
    let end = rest.find(|c: char| c == '{' || c == '<' || c == ' ' || c == '(')
        .unwrap_or(rest.len());
    Some(rest[..end].trim().to_string())
}

fn extract_enum_name(line: &str) -> Option<String> {
    let start = line.find("enum ")? + 5;
    let rest = &line[start..];
    let end = rest.find(|c: char| c == '{' || c == '<' || c == ' ')
        .unwrap_or(rest.len());
    Some(rest[..end].trim().to_string())
}

fn extract_trait_name(line: &str) -> Option<String> {
    let start = line.find("trait ")? + 6;
    let rest = &line[start..];
    let end = rest.find(|c: char| c == '{' || c == '<' || c == ' ' || c == ':')
        .unwrap_or(rest.len());
    Some(rest[..end].trim().to_string())
}

fn extract_impl_name(line: &str) -> Option<String> {
    let start = line.find("impl ")? + 5;
    let rest = &line[start..];
    let end = rest.find('{').unwrap_or(rest.len());
    Some(rest[..end].trim().to_string())
}

fn extract_ts_fn_name(line: &str) -> Option<String> {
    let start = line.find("function ")? + 9;
    let rest = &line[start..];
    let end = rest.find('(')?;
    Some(rest[..end].trim().to_string())
}

fn extract_class_name(line: &str) -> Option<String> {
    let start = line.find("class ")? + 6;
    let rest = &line[start..];
    let end = rest.find(|c: char| c == '{' || c == ' ' || c == '<')
        .unwrap_or(rest.len());
    Some(rest[..end].trim().to_string())
}

fn extract_interface_name(line: &str) -> Option<String> {
    let start = line.find("interface ")? + 10;
    let rest = &line[start..];
    let end = rest.find(|c: char| c == '{' || c == ' ' || c == '<')
        .unwrap_or(rest.len());
    Some(rest[..end].trim().to_string())
}

fn extract_type_name(line: &str) -> Option<String> {
    let start = line.find("type ")? + 5;
    let rest = &line[start..];
    let end = rest.find(|c: char| c == '=' || c == '<' || c == ' ')
        .unwrap_or(rest.len());
    Some(rest[..end].trim().to_string())
}

fn extract_arrow_fn_name(line: &str) -> Option<String> {
    let end = line.find(" =")?;
    let start = line.rfind(|c: char| c == ' ' || c == '\t').map(|i| i + 1).unwrap_or(0);
    Some(line[start..end].trim().to_string())
}

fn extract_py_fn_name(line: &str) -> Option<String> {
    let start = if line.starts_with("async def ") {
        10
    } else {
        4
    };
    let rest = &line[start..];
    let end = rest.find('(')?;
    Some(rest[..end].trim().to_string())
}

fn extract_py_class_name(line: &str) -> Option<String> {
    let start = line.find("class ")? + 6;
    let rest = &line[start..];
    let end = rest.find(|c: char| c == '(' || c == ':')
        .unwrap_or(rest.len());
    Some(rest[..end].trim().to_string())
}

fn extract_go_fn_name(line: &str) -> Option<String> {
    let start = line.find("func ")? + 5;
    let rest = &line[start..];
    if rest.starts_with('(') {
        let method_end = rest.find(')')?;
        let after_receiver = &rest[method_end + 1..].trim();
        let name_end = after_receiver.find('(')?;
        return Some(after_receiver[..name_end].trim().to_string());
    }
    let end = rest.find('(')?;
    Some(rest[..end].trim().to_string())
}

fn extract_go_type_name(line: &str) -> Option<String> {
    let start = line.find("type ")? + 5;
    let rest = &line[start..];
    let end = rest.find(' ')?;
    Some(rest[..end].trim().to_string())
}

fn extract_java_class_name(line: &str) -> Option<String> {
    let start = line.find("class ")? + 6;
    let rest = &line[start..];
    let end = rest.find(|c: char| c == '{' || c == ' ' || c == '<')
        .unwrap_or(rest.len());
    Some(rest[..end].trim().to_string())
}

fn extract_java_interface_name(line: &str) -> Option<String> {
    let start = line.find("interface ")? + 10;
    let rest = &line[start..];
    let end = rest.find(|c: char| c == '{' || c == ' ' || c == '<')
        .unwrap_or(rest.len());
    Some(rest[..end].trim().to_string())
}

fn extract_c_fn_name(line: &str) -> Option<String> {
    let paren = line.find('(')?;
    let before_paren = &line[..paren];
    let words: Vec<&str> = before_paren.split_whitespace().collect();
    words.last().map(|s| s.to_string())
}

fn extract_c_struct_name(line: &str) -> Option<String> {
    if line.contains("typedef") {
        let parts: Vec<&str> = line.split_whitespace().collect();
        return parts.get(2).map(|s| s.trim_end_matches('{').to_string());
    }
    let start = line.find("struct ")? + 7;
    let rest = &line[start..];
    let end = rest.find(|c: char| c == '{' || c == ' ')
        .unwrap_or(rest.len());
    Some(rest[..end].trim().to_string())
}
