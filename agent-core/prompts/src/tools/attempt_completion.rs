#![deny(clippy::all)]
#![forbid(unsafe_code)]

use crate::tools::spec::{ToolId, ToolSpec, ToolSpecParameter};
use crate::types::ModelFamily;

pub fn attempt_completion_variants() -> Vec<ToolSpec> {
    vec![generic_variant()]
}

fn generic_variant() -> ToolSpec {
    ToolSpec::new(ToolId::AttemptCompletion, ModelFamily::Generic)
        .with_name("attempt_completion")
        .with_description(
            "Signal that you believe the task is complete and present the final result. \
             Use this tool once you have fully accomplished the user's request. \
             Provide a comprehensive summary of what was done and any relevant outcomes. \
             If there are follow-up actions the user might want to take, mention them. \
             Do NOT use this if there are remaining steps or if verification is needed.",
        )
        .with_parameter(result_parameter())
        .with_parameter(command_parameter())
}

fn result_parameter() -> ToolSpecParameter {
    ToolSpecParameter::new(
        "result",
        "A comprehensive summary of what was accomplished. \
         Include key changes made, files modified, and any important outcomes. \
         Be specific but concise. Mention any potential follow-up actions.",
    )
    .with_usage("I have implemented the feature as requested...")
}

fn command_parameter() -> ToolSpecParameter {
    ToolSpecParameter::new(
        "command",
        "An optional command the user can run to verify or use the result. \
         For example, a command to start a server or run tests.",
    )
    .optional()
    .with_usage("npm run test")
}
