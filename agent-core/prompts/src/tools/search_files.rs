#![deny(clippy::all)]
#![forbid(unsafe_code)]

use crate::tools::spec::{task_progress_parameter, ToolId, ToolSpec, ToolSpecParameter};
use crate::types::ModelFamily;

pub fn search_files_variants() -> Vec<ToolSpec> {
    vec![generic_variant()]
}

fn generic_variant() -> ToolSpec {
    ToolSpec::new(ToolId::SearchFiles, ModelFamily::Generic)
        .with_name("search_files")
        .with_description(
            "Request to search for files containing a specific regex pattern. \
             Results show matching lines with context. \
             Use this to find code patterns, function definitions, or specific text across the codebase. \
             The search is recursive and respects .gitignore patterns.",
        )
        .with_parameter(path_parameter())
        .with_parameter(regex_parameter())
        .with_parameter(file_pattern_parameter())
        .with_parameter(task_progress_parameter())
}

fn path_parameter() -> ToolSpecParameter {
    ToolSpecParameter::new(
        "path",
        "The directory to search in (relative to {{CWD}}). Use '.' for current directory.",
    )
    .with_usage("src/")
}

fn regex_parameter() -> ToolSpecParameter {
    ToolSpecParameter::new(
        "regex",
        "The regular expression pattern to search for. Uses Rust regex syntax. \
         Case-sensitive by default. Use (?i) prefix for case-insensitive search.",
    )
    .with_usage("fn\\s+main")
}

fn file_pattern_parameter() -> ToolSpecParameter {
    ToolSpecParameter::new(
        "file_pattern",
        "Glob pattern to filter which files to search. Examples: *.rs, *.{ts,tsx}, **/*.py",
    )
    .optional()
    .with_usage("*.rs")
}
