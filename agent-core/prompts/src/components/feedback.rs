#![deny(clippy::all)]
#![forbid(unsafe_code)]

use crate::types::SystemPromptContext;
use crate::variant::PromptVariant;

pub async fn get_feedback(
    _variant: PromptVariant,
    context: SystemPromptContext,
) -> Option<String> {
    if !context.focus_chain_enabled {
        return None;
    }

    let content = r#"If the user asks for help or wants to give feedback inform them of the following:
- To give feedback, users should report the issue using the /reportbug slash command in the chat.

When the user directly asks about the agent (eg 'can it do...', 'does it have...') or asks in second person (eg 'are you able...', 'can you do...'), first use the web_fetch tool to gather information to answer the question from the documentation."#;

    Some(content.to_string())
}
