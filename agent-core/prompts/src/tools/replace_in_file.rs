#![deny(clippy::all)]
#![forbid(unsafe_code)]

use crate::tools::spec::{task_progress_parameter, ToolId, ToolSpec, ToolSpecParameter};
use crate::types::ModelFamily;

pub fn replace_in_file_variants() -> Vec<ToolSpec> {
    vec![generic_variant()]
}

fn generic_variant() -> ToolSpec {
    ToolSpec::new(ToolId::ReplaceInFile, ModelFamily::Generic)
        .with_name("replace_in_file")
        .with_description(
            "Request to replace sections of content in an existing file using SEARCH/REPLACE blocks. \
             This tool makes targeted changes without rewriting the entire file. \
             Each SEARCH block must match EXACTLY - including whitespace, indentation, and line breaks. \
             Use this for making specific changes to existing files.",
        )
        .with_parameter(path_parameter())
        .with_parameter(diff_parameter())
        .with_parameter(task_progress_parameter())
}

fn path_parameter() -> ToolSpecParameter {
    ToolSpecParameter::new(
        "path",
        "The path of the file to modify (relative to the current working directory {{CWD}}){{MULTI_ROOT_HINT}}",
    )
    .with_usage("File path here")
}

fn diff_parameter() -> ToolSpecParameter {
    ToolSpecParameter::new(
        "diff",
        "One or more SEARCH/REPLACE blocks. The SEARCH section must match the existing \
         content EXACTLY (including whitespace). The REPLACE section contains the new content. \
         Multiple blocks can be used to make multiple changes in one operation.",
    )
    .with_usage(
        "<<<<<<< SEARCH\nold content here\n=======\nnew content here\n>>>>>>> REPLACE",
    )
}
