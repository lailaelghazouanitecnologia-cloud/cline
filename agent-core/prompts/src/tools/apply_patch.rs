#![deny(clippy::all)]
#![forbid(unsafe_code)]

use crate::tools::spec::{task_progress_parameter, ToolId, ToolSpec, ToolSpecParameter};
use crate::types::ModelFamily;

pub fn apply_patch_variants() -> Vec<ToolSpec> {
    vec![generic_variant()]
}

fn generic_variant() -> ToolSpec {
    ToolSpec::new(ToolId::ApplyPatch, ModelFamily::Generic)
        .with_name("apply_patch")
        .with_description(
            "Request to apply a unified diff patch to modify files. \
             This tool can make changes to multiple files in a single operation. \
             Use standard unified diff format with --- and +++ headers for each file. \
             Changes are applied atomically - if any hunk fails, all changes are rolled back.",
        )
        .with_parameter(patch_parameter())
        .with_parameter(task_progress_parameter())
}

fn patch_parameter() -> ToolSpecParameter {
    ToolSpecParameter::new(
        "patch",
        "A unified diff patch in standard format. Each file section starts with \
         '--- a/path' and '+++ b/path' headers, followed by hunks with @@ markers. \
         Context lines start with space, removed lines with -, added lines with +.",
    )
    .with_usage("--- a/file.txt\n+++ b/file.txt\n@@ -1,3 +1,3 @@\n line1\n-old line\n+new line\n line3")
}
