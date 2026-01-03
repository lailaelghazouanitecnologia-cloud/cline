#![deny(clippy::all)]
#![forbid(unsafe_code)]

use crate::tools::spec::{task_progress_parameter, ToolId, ToolSpec, ToolSpecParameter};
use crate::types::ModelFamily;

pub fn read_file_variants() -> Vec<ToolSpec> {
    vec![generic_variant(), next_gen_variant(), gpt5_variant()]
}

fn generic_variant() -> ToolSpec {
    ToolSpec::new(ToolId::ReadFile, ModelFamily::Generic)
        .with_name("read_file")
        .with_description(
            "Request to read the contents of a file at the specified path. \
             Use this when you need to examine the contents of an existing file \
             you do not know the contents of, for example to analyze code, review \
             text files, or extract information from configuration files. \
             Automatically extracts raw text from PDF and DOCX files. \
             May not be suitable for other types of binary files, as it returns \
             the raw content as a string. Do NOT use this tool to list the contents \
             of a directory. Only use this tool on files.",
        )
        .with_parameter(path_parameter())
        .with_parameter(task_progress_parameter())
}

fn next_gen_variant() -> ToolSpec {
    ToolSpec::new(ToolId::ReadFile, ModelFamily::ClaudeNextGen)
        .with_name("read_file")
        .with_description(
            "Request to read the contents of a file at the specified path. \
             Use this when you need to examine the contents of an existing file \
             you do not know the contents of, for example to analyze code, review \
             text files, or extract information from configuration files. \
             Automatically extracts raw text from PDF and DOCX files. \
             May not be suitable for other types of binary files. \
             Do NOT use this tool to list directory contents.",
        )
        .with_parameter(path_parameter())
        .with_parameter(task_progress_parameter())
}

fn gpt5_variant() -> ToolSpec {
    ToolSpec::new(ToolId::ReadFile, ModelFamily::Gpt5)
        .with_name("read_file")
        .with_description(
            "Request to read the contents of a file at the specified path. \
             Use this when you need to examine the contents of an existing file \
             you do not know the contents of, for example to analyze code, review \
             text files, or extract information from configuration files. \
             Automatically extracts raw text from PDF and DOCX files. \
             Do NOT use this tool to list directory contents.",
        )
        .with_parameter(path_parameter())
        .with_parameter(task_progress_parameter())
}

fn path_parameter() -> ToolSpecParameter {
    ToolSpecParameter::new(
        "path",
        "The path of the file to read (relative to the current working directory {{CWD}}){{MULTI_ROOT_HINT}}",
    )
    .with_usage("File path here")
}
