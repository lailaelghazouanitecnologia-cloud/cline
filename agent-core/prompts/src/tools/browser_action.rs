#![deny(clippy::all)]
#![forbid(unsafe_code)]

use crate::tools::spec::{ToolId, ToolSpec, ToolSpecParameter};
use crate::types::{ModelFamily, SystemPromptContext};

pub fn browser_action_variants() -> Vec<ToolSpec> {
    vec![generic_variant()]
}

fn generic_variant() -> ToolSpec {
    ToolSpec::new(ToolId::BrowserAction, ModelFamily::Generic)
        .with_name("browser_action")
        .with_description(
            "Request to interact with a browser for web navigation and testing. \
             Actions include launching the browser, navigating to URLs, clicking elements, \
             typing text, scrolling, and taking screenshots. \
             The browser runs in a controlled environment with a fixed viewport size. \
             Use this to test web applications or verify web-based implementations.",
        )
        .with_context_requirements(requires_browser)
        .with_parameter(action_parameter())
        .with_parameter(url_parameter())
        .with_parameter(coordinate_parameter())
        .with_parameter(text_parameter())
}

fn requires_browser(context: &SystemPromptContext) -> bool {
    context.supports_browser
}

fn action_parameter() -> ToolSpecParameter {
    ToolSpecParameter::new(
        "action",
        "The browser action to perform: launch, navigate, click, type, scroll_down, \
         scroll_up, screenshot, close. Each action has specific required parameters.",
    )
    .with_usage("navigate")
}

fn url_parameter() -> ToolSpecParameter {
    ToolSpecParameter::new(
        "url",
        "The URL to navigate to. Required for 'navigate' action. \
         Must be a valid URL including protocol (http:// or https://).",
    )
    .optional()
    .with_usage("https://example.com")
    .with_context_requirements(|_| true)
}

fn coordinate_parameter() -> ToolSpecParameter {
    ToolSpecParameter::new(
        "coordinate",
        "The x,y coordinate to click. Required for 'click' action. \
         Format: 'x,y' where x and y are pixel coordinates from top-left corner.",
    )
    .optional()
    .with_usage("100,200")
}

fn text_parameter() -> ToolSpecParameter {
    ToolSpecParameter::new(
        "text",
        "The text to type. Required for 'type' action. \
         Special keys can be included using {key} syntax (e.g., {Enter}, {Tab}).",
    )
    .optional()
    .with_usage("Hello World")
}
