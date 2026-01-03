#![deny(clippy::all)]
#![forbid(unsafe_code)]

use crate::tools::spec::{task_progress_parameter, ToolId, ToolSpec, ToolSpecParameter};
use crate::types::ModelFamily;

pub fn execute_command_variants() -> Vec<ToolSpec> {
    vec![generic_variant(), next_gen_variant()]
}

fn generic_variant() -> ToolSpec {
    ToolSpec::new(ToolId::ExecuteCommand, ModelFamily::Generic)
        .with_name("execute_command")
        .with_description(
            "Request to execute a CLI command on the system. \
             Use this when you need to perform system operations or run specific commands \
             to accomplish tasks. Commands are executed in the current working directory. \
             You must tailor your command to the user's system and provide a clear explanation \
             of what the command does. Prefer using && to chain commands that depend on each other. \
             The command will be run in a shell environment.",
        )
        .with_parameter(command_parameter())
        .with_parameter(requires_approval_parameter())
        .with_parameter(task_progress_parameter())
}

fn next_gen_variant() -> ToolSpec {
    ToolSpec::new(ToolId::ExecuteCommand, ModelFamily::ClaudeNextGen)
        .with_name("execute_command")
        .with_description(
            "Execute a CLI command on the system. \
             Commands run in the current working directory. \
             Tailor commands to the user's system. \
             Use && to chain dependent commands. \
             Consider using background execution for long-running processes.",
        )
        .with_parameter(command_parameter())
        .with_parameter(requires_approval_parameter())
        .with_parameter(background_parameter())
        .with_parameter(task_progress_parameter())
}

fn command_parameter() -> ToolSpecParameter {
    ToolSpecParameter::new(
        "command",
        "The CLI command to execute. Must be valid for the current operating system and shell. \
         Complex commands should be broken into multiple calls or chained with &&.",
    )
    .with_usage("npm install")
}

fn requires_approval_parameter() -> ToolSpecParameter {
    ToolSpecParameter::new(
        "requires_approval",
        "Set to true for commands that could have significant effects \
         (installing packages, modifying system settings, etc.). \
         When true, the user will be prompted to approve before execution.",
    )
    .optional()
    .with_usage("true")
}

fn background_parameter() -> ToolSpecParameter {
    ToolSpecParameter::new(
        "background",
        "Set to true to run the command in the background. \
         Useful for long-running processes like servers or watchers. \
         Background processes can be checked later for output.",
    )
    .optional()
    .with_usage("false")
}
