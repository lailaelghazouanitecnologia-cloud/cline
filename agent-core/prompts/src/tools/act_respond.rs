#![deny(clippy::all)]
#![forbid(unsafe_code)]

use crate::tools::spec::{ToolId, ToolSpec, ToolSpecParameter};
use crate::types::ModelFamily;

pub fn act_respond_variants() -> Vec<ToolSpec> {
    vec![generic_variant()]
}

fn generic_variant() -> ToolSpec {
    ToolSpec::new(ToolId::ActRespond, ModelFamily::Generic)
        .with_name("act_mode_respond")
        .with_description(
            "Respond to the user in act mode. Use this to provide updates, \
             explain what you're doing, or communicate during task execution. \
             This is for informational responses that don't require user input \
             or don't signal task completion.",
        )
        .with_parameter(response_parameter())
}

fn response_parameter() -> ToolSpecParameter {
    ToolSpecParameter::new(
        "response",
        "Your response or update to the user. \
         Keep it focused and relevant to the current task. \
         Use this for status updates, explanations, or interim results.",
    )
    .with_usage("I'm now analyzing the codebase structure...")
}
