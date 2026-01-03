#![deny(clippy::all)]
#![forbid(unsafe_code)]

use crate::tools::spec::{task_progress_parameter, ToolId, ToolSpec, ToolSpecParameter};
use crate::types::ModelFamily;

pub fn list_files_variants() -> Vec<ToolSpec> {
    vec![generic_variant()]
}

fn generic_variant() -> ToolSpec {
    ToolSpec::new(ToolId::ListFiles, ModelFamily::Generic)
        .with_name("list_files")
        .with_description(
            "Request to list files and directories at the specified path. \
             Returns a hierarchical view with directories marked. \
             If recursive is true, lists all nested contents (can be slow for large directories). \
             Hidden files (starting with .) are included by default. \
             Respects .gitignore patterns.",
        )
        .with_parameter(path_parameter())
        .with_parameter(recursive_parameter())
        .with_parameter(task_progress_parameter())
}

fn path_parameter() -> ToolSpecParameter {
    ToolSpecParameter::new(
        "path",
        "The directory path to list (relative to {{CWD}}). Use '.' for current directory.",
    )
    .with_usage("src/")
}

fn recursive_parameter() -> ToolSpecParameter {
    ToolSpecParameter::new(
        "recursive",
        "If true, recursively list all files in subdirectories. \
         Default is false. Use with caution on large directories.",
    )
    .optional()
    .with_usage("false")
}
