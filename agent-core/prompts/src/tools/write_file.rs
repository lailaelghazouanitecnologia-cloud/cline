#![deny(clippy::all)]
#![forbid(unsafe_code)]

use crate::tools::spec::{task_progress_parameter, ToolId, ToolSpec, ToolSpecParameter};
use crate::types::ModelFamily;

pub fn write_file_variants() -> Vec<ToolSpec> {
    vec![generic_variant(), next_gen_variant(), gpt5_variant()]
}

fn generic_variant() -> ToolSpec {
    ToolSpec::new(ToolId::WriteFile, ModelFamily::Generic)
        .with_name("write_to_file")
        .with_description(
            "Request to write content to a file at the specified path. \
             If the file exists, it will be overwritten with the provided content. \
             If the file doesn't exist, it will be created. \
             This tool will automatically create any directories needed to write the file.",
        )
        .with_parameter(path_parameter())
        .with_parameter(content_parameter())
        .with_parameter(task_progress_parameter())
}

fn next_gen_variant() -> ToolSpec {
    ToolSpec::new(ToolId::WriteFile, ModelFamily::ClaudeNextGen)
        .with_name("write_to_file")
        .with_description(
            "[IMPORTANT: Always output the absolutePath first] \
             Request to write content to a file at the specified path. \
             If the file exists, it will be overwritten with the provided content. \
             If the file doesn't exist, it will be created. \
             This tool will automatically create any directories needed.",
        )
        .with_parameter(
            ToolSpecParameter::new(
                "absolutePath",
                "The absolute path to the file to write to.",
            )
            .with_usage("/path/to/file"),
        )
        .with_parameter(
            ToolSpecParameter::new(
                "content",
                "After providing the path so a file can be created, then use this to provide the content to write.",
            )
            .with_usage("Your file content here"),
        )
        .with_parameter(task_progress_parameter())
}

fn gpt5_variant() -> ToolSpec {
    ToolSpec::new(ToolId::WriteFile, ModelFamily::Gpt5)
        .with_name("write_to_file")
        .with_description(
            "[IMPORTANT: Always output the absolutePath first] \
             Request to write content to a file at the specified path. \
             If the file exists, it will be overwritten. If not, it will be created. \
             Directories are created automatically as needed.",
        )
        .with_parameter(
            ToolSpecParameter::new(
                "absolutePath",
                "The absolute path to the file to write to.",
            )
            .with_usage("/path/to/file"),
        )
        .with_parameter(
            ToolSpecParameter::new(
                "content",
                "After providing the path, provide the content to write to the file.",
            )
            .with_usage("Your file content here"),
        )
        .with_parameter(task_progress_parameter())
}

fn path_parameter() -> ToolSpecParameter {
    ToolSpecParameter::new(
        "path",
        "The path of the file to write to (relative to the current working directory {{CWD}}){{MULTI_ROOT_HINT}}",
    )
    .with_usage("File path here")
}

fn content_parameter() -> ToolSpecParameter {
    ToolSpecParameter::new(
        "content",
        "The content to write to the file. ALWAYS provide the COMPLETE intended content \
         of the file, without any truncation or omissions. You MUST include ALL parts of \
         the file, even if they haven't been modified.",
    )
    .with_usage("Your file content here")
}
