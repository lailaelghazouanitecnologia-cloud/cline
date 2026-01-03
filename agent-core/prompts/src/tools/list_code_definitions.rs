#![deny(clippy::all)]
#![forbid(unsafe_code)]

use crate::tools::spec::{task_progress_parameter, ToolId, ToolSpec, ToolSpecParameter};
use crate::types::ModelFamily;

pub fn list_code_definitions_variants() -> Vec<ToolSpec> {
    vec![generic_variant()]
}

fn generic_variant() -> ToolSpec {
    ToolSpec::new(ToolId::ListCodeDefinitions, ModelFamily::Generic)
        .with_name("list_code_definition_names")
        .with_description(
            "Request to list all code definition names (classes, functions, methods, etc.) \
             in the specified directory. This tool uses tree-sitter to parse source files \
             and extract symbol information. Useful for understanding codebase structure \
             without reading entire files. Results are grouped by file and include line numbers.",
        )
        .with_parameter(path_parameter())
        .with_parameter(task_progress_parameter())
}

fn path_parameter() -> ToolSpecParameter {
    ToolSpecParameter::new(
        "path",
        "The directory path to analyze (relative to {{CWD}}). \
         Will recursively scan all recognized source files.",
    )
    .with_usage("src/")
}
