#![deny(clippy::all)]
#![forbid(unsafe_code)]

use crate::tools::spec::{ToolId, ToolSpec, ToolSpecParameter};
use crate::types::ModelFamily;

pub fn ask_followup_variants() -> Vec<ToolSpec> {
    vec![generic_variant()]
}

fn generic_variant() -> ToolSpec {
    ToolSpec::new(ToolId::AskFollowup, ModelFamily::Generic)
        .with_name("ask_followup_question")
        .with_description(
            "Ask the user a clarifying question when you need more information to proceed. \
             Use this tool when the task is ambiguous, requires a decision, \
             or when you need confirmation before making significant changes. \
             The question should be specific and actionable. \
             Optionally provide suggested answers to help the user respond quickly.",
        )
        .with_parameter(question_parameter())
        .with_parameter(suggestions_parameter())
}

fn question_parameter() -> ToolSpecParameter {
    ToolSpecParameter::new(
        "question",
        "The clarifying question to ask the user. Be specific and provide context. \
         Explain why you need this information to proceed.",
    )
    .with_usage("Which testing framework would you like me to use: Jest or Vitest?")
}

fn suggestions_parameter() -> ToolSpecParameter {
    ToolSpecParameter::new(
        "suggestions",
        "A list of suggested answers to help the user respond quickly. \
         These appear as clickable options. Keep them concise.",
    )
    .optional()
    .with_usage("[\"Jest\", \"Vitest\", \"Let me decide later\"]")
}
