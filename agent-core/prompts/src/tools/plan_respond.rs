#![deny(clippy::all)]
#![forbid(unsafe_code)]

use crate::tools::spec::{ToolId, ToolSpec, ToolSpecParameter};
use crate::types::ModelFamily;

pub fn plan_respond_variants() -> Vec<ToolSpec> {
    vec![generic_variant()]
}

fn generic_variant() -> ToolSpec {
    ToolSpec::new(ToolId::PlanRespond, ModelFamily::Generic)
        .with_name("plan_mode_respond")
        .with_description(
            "Respond to the user in plan mode. In this mode, you analyze the task \
             and develop a detailed plan before taking action. \
             Use this to outline your approach, identify potential challenges, \
             and get user approval before proceeding with implementation. \
             The plan should be clear, actionable, and address all aspects of the task.",
        )
        .with_parameter(response_parameter())
        .with_parameter(options_parameter())
}

fn response_parameter() -> ToolSpecParameter {
    ToolSpecParameter::new(
        "response",
        "Your response to the user including the analysis and proposed plan. \
         Structure it clearly with numbered steps. \
         Include assumptions, potential issues, and alternatives if applicable.",
    )
    .with_usage("Based on my analysis, here's my proposed approach:\n\n1. First...")
}

fn options_parameter() -> ToolSpecParameter {
    ToolSpecParameter::new(
        "options",
        "A list of options for the user to choose from, such as 'Approve', 'Modify', 'Cancel'. \
         These appear as buttons for quick responses.",
    )
    .optional()
    .with_usage("[\"Approve plan\", \"Let me modify it\", \"Cancel\"]")
}
