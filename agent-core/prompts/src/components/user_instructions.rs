#![deny(clippy::all)]
#![forbid(unsafe_code)]

use crate::types::SystemPromptContext;
use crate::variant::PromptVariant;

pub async fn get_user_instructions(_variant: PromptVariant, context: SystemPromptContext) -> Option<String> {
    let mut sections = Vec::new();

    if let Some(ref global) = context.global_rules {
        if !global.trim().is_empty() {
            sections.push(format!("# Global Instructions\n\n{}", global.trim()));
        }
    }

    if let Some(ref local) = context.local_rules {
        if !local.trim().is_empty() {
            sections.push(format!("# Project Instructions (.clinerules)\n\n{}", local.trim()));
        }
    }

    if let Some(ref cursor) = context.cursor_rules {
        if !cursor.trim().is_empty() {
            sections.push(format!("# Cursor Rules (.cursorrules)\n\n{}", cursor.trim()));
        }
    }

    if let Some(ref windsurf) = context.windsurf_rules {
        if !windsurf.trim().is_empty() {
            sections.push(format!("# Windsurf Rules (.windsurfrules)\n\n{}", windsurf.trim()));
        }
    }

    if let Some(ref custom) = context.custom_instructions {
        if !custom.trim().is_empty() {
            sections.push(format!("# Custom Instructions\n\n{}", custom.trim()));
        }
    }

    if let Some(ref cline_ignore) = context.cline_ignore {
        if !cline_ignore.trim().is_empty() {
            sections.push(format!(
                "# Ignored Paths (.clineignore)\n\n\
                The following paths should be ignored and not modified:\n\n{}",
                cline_ignore.trim()
            ));
        }
    }

    if sections.is_empty() {
        return None;
    }

    let mut content = String::from("USER INSTRUCTIONS\n\n");
    content.push_str("The following instructions have been provided by the user and/or project configuration. ");
    content.push_str("Follow these instructions carefully as they may contain important context, preferences, or constraints.\n\n");
    content.push_str(&sections.join("\n\n---\n\n"));

    Some(content)
}
