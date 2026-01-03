#![deny(clippy::all)]
#![forbid(unsafe_code)]

use crate::tools::spec::{task_progress_parameter, ToolId, ToolSpec, ToolSpecParameter};
use crate::types::{ModelFamily, SystemPromptContext};

pub fn web_fetch_variants() -> Vec<ToolSpec> {
    vec![generic_variant()]
}

fn generic_variant() -> ToolSpec {
    ToolSpec::new(ToolId::WebFetch, ModelFamily::Generic)
        .with_name("web_fetch")
        .with_description(
            "Request to fetch content from a URL. Returns the page content as text. \
             HTML is automatically converted to readable text. \
             Useful for retrieving documentation, API responses, or web page content. \
             Respects robots.txt and has rate limiting.",
        )
        .with_context_requirements(requires_web_tools)
        .with_parameter(url_parameter())
        .with_parameter(task_progress_parameter())
}

fn requires_web_tools(context: &SystemPromptContext) -> bool {
    context.web_tools_enabled
}

fn url_parameter() -> ToolSpecParameter {
    ToolSpecParameter::new(
        "url",
        "The URL to fetch content from. Must be a valid URL with protocol (http:// or https://).",
    )
    .with_usage("https://docs.example.com/api")
}
