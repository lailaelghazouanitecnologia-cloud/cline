#![deny(clippy::all)]
#![forbid(unsafe_code)]

use crate::types::SystemPromptContext;
use crate::variant::PromptVariant;

pub async fn get_agent_role(_variant: PromptVariant, context: SystemPromptContext) -> Option<String> {
    let mut content = String::from(
        "You are Cline, a highly skilled software engineer with extensive knowledge in \
        many programming languages, frameworks, design patterns, and best practices.",
    );

    if context.is_cli_subagent {
        content.push_str(
            "\n\nYou are running as a CLI subagent, spawned by a parent agent to handle \
            a specific subtask. Focus on completing your assigned task efficiently and \
            report back with clear results.",
        );
    }

    Some(content)
}
