#![deny(clippy::all)]
#![forbid(unsafe_code)]

use crate::tools::spec::{task_progress_parameter, ToolId, ToolSpec, ToolSpecParameter};
use crate::types::{ModelFamily, SystemPromptContext};

pub fn web_search_variants() -> Vec<ToolSpec> {
    vec![generic_variant()]
}

fn generic_variant() -> ToolSpec {
    ToolSpec::new(ToolId::WebSearch, ModelFamily::Generic)
        .with_name("web_search")
        .with_description(
            "Request to search the web for information. Returns a list of relevant results \
             with titles, snippets, and URLs. Use this to find current information, \
             documentation, or solutions to technical problems. \
             Results are from major search engines.",
        )
        .with_context_requirements(requires_web_tools)
        .with_parameter(query_parameter())
        .with_parameter(task_progress_parameter())
}

fn requires_web_tools(context: &SystemPromptContext) -> bool {
    context.web_tools_enabled
}

fn query_parameter() -> ToolSpecParameter {
    ToolSpecParameter::new(
        "query",
        "The search query. Be specific to get better results. \
         Include relevant keywords, technology names, or error messages.",
    )
    .with_usage("rust async trait implementation")
}
