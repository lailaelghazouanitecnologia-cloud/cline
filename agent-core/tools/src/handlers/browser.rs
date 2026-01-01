#![deny(clippy::all)]

use crate::context::ToolContext;
use crate::handler::{ToolFuture, ToolHandler};
use crate::spec::{ToolCall, ToolOutput, ToolSpec};
use agent_common::AgentResult;
use base64::Engine;
use headless_chrome::browser::tab::point::Point;
use headless_chrome::{Browser, LaunchOptions, Tab};
use std::sync::Arc;

pub struct BrowserHandler {
    browser: Option<Arc<Browser>>,
    current_tab: Option<Arc<Tab>>,
}

impl BrowserHandler {
    pub fn new() -> Self {
        Self {
            browser: None,
            current_tab: None,
        }
    }

    fn spec() -> ToolSpec {
        ToolSpec::new(
            "browser_action",
            "Perform browser automation actions: navigate, click, type, screenshot, scroll.",
        )
        .with_parameter("action", "string", "Action: launch, navigate, click, type, screenshot, scroll, close", true)
        .with_parameter("url", "string", "URL to navigate to (for navigate action)", false)
        .with_parameter("selector", "string", "CSS selector for element (for click/type)", false)
        .with_parameter("text", "string", "Text to type (for type action)", false)
        .with_parameter("direction", "string", "Scroll direction: up, down (for scroll)", false)
        .with_parameter("coordinate", "string", "x,y coordinates for click (alternative to selector)", false)
    }

    fn get_or_launch_browser(&mut self) -> AgentResult<Arc<Browser>> {
        if let Some(ref browser) = self.browser {
            return Ok(browser.clone());
        }

        let options = LaunchOptions::default_builder()
            .headless(true)
            .window_size(Some((1280, 720)))
            .build()
            .map_err(|e| agent_common::AgentError::tool_execution("browser", e.to_string()))?;

        let browser = Browser::new(options)
            .map_err(|e| agent_common::AgentError::tool_execution("browser", e.to_string()))?;

        let browser = Arc::new(browser);
        self.browser = Some(browser.clone());
        Ok(browser)
    }

    fn get_or_create_tab(&mut self) -> AgentResult<Arc<Tab>> {
        if let Some(ref tab) = self.current_tab {
            return Ok(tab.clone());
        }

        let browser = self.get_or_launch_browser()?;
        let tab = browser
            .new_tab()
            .map_err(|e| agent_common::AgentError::tool_execution("browser", e.to_string()))?;

        self.current_tab = Some(tab.clone());
        Ok(tab)
    }

    fn execute_action(
        &mut self,
        action: &str,
        url: Option<String>,
        selector: Option<String>,
        text: Option<String>,
        direction: Option<String>,
        coordinate: Option<String>,
    ) -> AgentResult<ToolOutput> {
        match action {
            "launch" => self.action_launch(),
            "navigate" => self.action_navigate(url),
            "click" => self.action_click(selector, coordinate),
            "type" => self.action_type(selector, text),
            "screenshot" => self.action_screenshot(),
            "scroll" => self.action_scroll(direction),
            "close" => self.action_close(),
            "get_text" => self.action_get_text(selector),
            _ => Ok(ToolOutput::failure(format!("Unknown action: {}", action))),
        }
    }

    fn action_launch(&mut self) -> AgentResult<ToolOutput> {
        self.get_or_launch_browser()?;
        self.get_or_create_tab()?;
        Ok(ToolOutput::success("Browser launched successfully"))
    }

    fn action_navigate(&mut self, url: Option<String>) -> AgentResult<ToolOutput> {
        let url = match url {
            Some(u) => u,
            None => return Ok(ToolOutput::failure("URL is required for navigate action")),
        };

        let tab = self.get_or_create_tab()?;
        tab.navigate_to(&url)
            .map_err(|e| agent_common::AgentError::tool_execution("browser", e.to_string()))?;

        tab.wait_until_navigated()
            .map_err(|e| agent_common::AgentError::tool_execution("browser", e.to_string()))?;

        Ok(ToolOutput::success(format!("Navigated to {}", url)))
    }

    fn action_click(&mut self, selector: Option<String>, coordinate: Option<String>) -> AgentResult<ToolOutput> {
        let tab = self.get_or_create_tab()?;

        if let Some(sel) = selector {
            let element = tab
                .wait_for_element(&sel)
                .map_err(|e| agent_common::AgentError::tool_execution("browser", e.to_string()))?;

            element
                .click()
                .map_err(|e| agent_common::AgentError::tool_execution("browser", e.to_string()))?;

            return Ok(ToolOutput::success(format!("Clicked element: {}", sel)));
        }

        if let Some(coord) = coordinate {
            let parts: Vec<&str> = coord.split(',').collect();
            if parts.len() != 2 {
                return Ok(ToolOutput::failure("Coordinate must be in format: x,y"));
            }

            let x: f64 = parts[0].trim().parse().unwrap_or(0.0);
            let y: f64 = parts[1].trim().parse().unwrap_or(0.0);

            tab.click_point(Point { x, y })
                .map_err(|e| agent_common::AgentError::tool_execution("browser", e.to_string()))?;

            return Ok(ToolOutput::success(format!("Clicked at ({}, {})", x, y)));
        }

        Ok(ToolOutput::failure("Either selector or coordinate is required for click"))
    }

    fn action_type(&mut self, selector: Option<String>, text: Option<String>) -> AgentResult<ToolOutput> {
        let text = match text {
            Some(t) => t,
            None => return Ok(ToolOutput::failure("Text is required for type action")),
        };

        let tab = self.get_or_create_tab()?;

        if let Some(sel) = selector {
            let element = tab
                .wait_for_element(&sel)
                .map_err(|e| agent_common::AgentError::tool_execution("browser", e.to_string()))?;

            element
                .click()
                .map_err(|e| agent_common::AgentError::tool_execution("browser", e.to_string()))?;
        }

        tab.type_str(&text)
            .map_err(|e| agent_common::AgentError::tool_execution("browser", e.to_string()))?;

        Ok(ToolOutput::success(format!("Typed: {}", truncate(&text, 50))))
    }

    fn action_screenshot(&mut self) -> AgentResult<ToolOutput> {
        let tab = self.get_or_create_tab()?;

        let screenshot = tab
            .capture_screenshot(
                headless_chrome::protocol::cdp::Page::CaptureScreenshotFormatOption::Png,
                None,
                None,
                true,
            )
            .map_err(|e| agent_common::AgentError::tool_execution("browser", e.to_string()))?;

        let base64_data = base64::engine::general_purpose::STANDARD.encode(&screenshot);

        Ok(ToolOutput::success(format!(
            "Screenshot captured ({} bytes, base64 encoded)",
            base64_data.len()
        )))
    }

    fn action_scroll(&mut self, direction: Option<String>) -> AgentResult<ToolOutput> {
        let direction = direction.unwrap_or_else(|| "down".to_string());
        let tab = self.get_or_create_tab()?;

        let scroll_amount = match direction.as_str() {
            "up" => -500,
            "down" => 500,
            _ => 500,
        };

        tab.evaluate(
            &format!("window.scrollBy(0, {})", scroll_amount),
            false,
        )
        .map_err(|e| agent_common::AgentError::tool_execution("browser", e.to_string()))?;

        Ok(ToolOutput::success(format!("Scrolled {}", direction)))
    }

    fn action_close(&mut self) -> AgentResult<ToolOutput> {
        self.current_tab = None;
        self.browser = None;
        Ok(ToolOutput::success("Browser closed"))
    }

    fn action_get_text(&mut self, selector: Option<String>) -> AgentResult<ToolOutput> {
        let sel = match selector {
            Some(s) => s,
            None => return Ok(ToolOutput::failure("Selector is required for get_text")),
        };

        let tab = self.get_or_create_tab()?;

        let element = tab
            .wait_for_element(&sel)
            .map_err(|e| agent_common::AgentError::tool_execution("browser", e.to_string()))?;

        let text = element
            .get_inner_text()
            .map_err(|e| agent_common::AgentError::tool_execution("browser", e.to_string()))?;

        Ok(ToolOutput::success(text))
    }
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len])
    }
}

impl Default for BrowserHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolHandler for BrowserHandler {
    fn spec(&self) -> ToolSpec {
        Self::spec()
    }

    fn execute(&self, _context: &ToolContext, call: ToolCall) -> ToolFuture {
        let action = call.get_string("action").unwrap_or_default();
        let url = call.get_string("url");
        let selector = call.get_string("selector");
        let text = call.get_string("text");
        let direction = call.get_string("direction");
        let coordinate = call.get_string("coordinate");

        Box::pin(async move {
            if action.is_empty() {
                return Ok(ToolOutput::failure("Missing required parameter: action"));
            }

            let mut handler = BrowserHandler::new();
            handler.execute_action(&action, url, selector, text, direction, coordinate)
        })
    }

    fn is_dangerous(&self, _call: &ToolCall) -> bool {
        true
    }
}
