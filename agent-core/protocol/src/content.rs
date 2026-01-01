#![deny(clippy::all)]
#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    Text {
        text: String,
    },
    Image {
        source: ImageSource,
        alt_text: Option<String>,
    },
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    ToolResult {
        tool_use_id: String,
        content: ToolResultContent,
        is_error: bool,
    },
    Thinking {
        thinking: String,
        signature: Option<String>,
    },
    RedactedThinking {
        data: String,
    },
    Document {
        source: DocumentSource,
        title: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ImageSource {
    Base64 {
        media_type: String,
        data: String,
    },
    Url {
        url: String,
    },
    File {
        path: PathBuf,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ToolResultContent {
    Text(String),
    Blocks(Vec<ContentBlock>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DocumentSource {
    Base64 {
        media_type: String,
        data: String,
    },
    Url {
        url: String,
    },
    File {
        path: PathBuf,
    },
}

impl ContentBlock {
    pub fn text(content: impl Into<String>) -> Self {
        Self::Text { text: content.into() }
    }

    pub fn image_base64(media_type: impl Into<String>, data: impl Into<String>) -> Self {
        Self::Image {
            source: ImageSource::Base64 {
                media_type: media_type.into(),
                data: data.into(),
            },
            alt_text: None,
        }
    }

    pub fn image_url(url: impl Into<String>) -> Self {
        Self::Image {
            source: ImageSource::Url { url: url.into() },
            alt_text: None,
        }
    }

    pub fn image_file(path: impl Into<PathBuf>) -> Self {
        Self::Image {
            source: ImageSource::File { path: path.into() },
            alt_text: None,
        }
    }

    pub fn tool_use(id: impl Into<String>, name: impl Into<String>, input: serde_json::Value) -> Self {
        Self::ToolUse {
            id: id.into(),
            name: name.into(),
            input,
        }
    }

    pub fn tool_result(tool_use_id: impl Into<String>, content: impl Into<String>, is_error: bool) -> Self {
        Self::ToolResult {
            tool_use_id: tool_use_id.into(),
            content: ToolResultContent::Text(content.into()),
            is_error,
        }
    }

    pub fn tool_result_blocks(tool_use_id: impl Into<String>, blocks: Vec<ContentBlock>, is_error: bool) -> Self {
        Self::ToolResult {
            tool_use_id: tool_use_id.into(),
            content: ToolResultContent::Blocks(blocks),
            is_error,
        }
    }

    pub fn thinking(content: impl Into<String>) -> Self {
        Self::Thinking {
            thinking: content.into(),
            signature: None,
        }
    }

    pub fn thinking_with_signature(content: impl Into<String>, signature: impl Into<String>) -> Self {
        Self::Thinking {
            thinking: content.into(),
            signature: Some(signature.into()),
        }
    }

    pub fn redacted_thinking(data: impl Into<String>) -> Self {
        Self::RedactedThinking { data: data.into() }
    }

    pub fn is_text(&self) -> bool {
        matches!(self, Self::Text { .. })
    }

    pub fn is_image(&self) -> bool {
        matches!(self, Self::Image { .. })
    }

    pub fn is_tool_use(&self) -> bool {
        matches!(self, Self::ToolUse { .. })
    }

    pub fn is_tool_result(&self) -> bool {
        matches!(self, Self::ToolResult { .. })
    }

    pub fn is_thinking(&self) -> bool {
        matches!(self, Self::Thinking { .. } | Self::RedactedThinking { .. })
    }

    pub fn get_text(&self) -> Option<&str> {
        match self {
            Self::Text { text } => Some(text),
            _ => None,
        }
    }

    pub fn get_tool_use_id(&self) -> Option<&str> {
        match self {
            Self::ToolUse { id, .. } => Some(id),
            _ => None,
        }
    }

    pub fn get_tool_result_id(&self) -> Option<&str> {
        match self {
            Self::ToolResult { tool_use_id, .. } => Some(tool_use_id),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: MessageRole,
    pub content: Vec<ContentBlock>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    User,
    Assistant,
    System,
}

impl Message {
    pub fn user(content: Vec<ContentBlock>) -> Self {
        Self {
            role: MessageRole::User,
            content,
        }
    }

    pub fn assistant(content: Vec<ContentBlock>) -> Self {
        Self {
            role: MessageRole::Assistant,
            content,
        }
    }

    pub fn system(content: Vec<ContentBlock>) -> Self {
        Self {
            role: MessageRole::System,
            content,
        }
    }

    pub fn user_text(text: impl Into<String>) -> Self {
        Self::user(vec![ContentBlock::text(text)])
    }

    pub fn assistant_text(text: impl Into<String>) -> Self {
        Self::assistant(vec![ContentBlock::text(text)])
    }

    pub fn system_text(text: impl Into<String>) -> Self {
        Self::system(vec![ContentBlock::text(text)])
    }

    pub fn get_text_content(&self) -> String {
        self.content
            .iter()
            .filter_map(|b| b.get_text())
            .collect::<Vec<_>>()
            .join("")
    }

    pub fn has_images(&self) -> bool {
        self.content.iter().any(|b| b.is_image())
    }

    pub fn has_tool_use(&self) -> bool {
        self.content.iter().any(|b| b.is_tool_use())
    }

    pub fn has_tool_result(&self) -> bool {
        self.content.iter().any(|b| b.is_tool_result())
    }

    pub fn get_tool_uses(&self) -> Vec<(&str, &str, &serde_json::Value)> {
        self.content
            .iter()
            .filter_map(|b| match b {
                ContentBlock::ToolUse { id, name, input } => Some((id.as_str(), name.as_str(), input)),
                _ => None,
            })
            .collect()
    }

    pub fn add_content(&mut self, block: ContentBlock) {
        self.content.push(block);
    }

    pub fn add_text(&mut self, text: impl Into<String>) {
        self.content.push(ContentBlock::text(text));
    }

    pub fn add_image_base64(&mut self, media_type: impl Into<String>, data: impl Into<String>) {
        self.content.push(ContentBlock::image_base64(media_type, data));
    }
}

#[derive(Debug, Clone, Default)]
pub struct MessageBuilder {
    role: Option<MessageRole>,
    content: Vec<ContentBlock>,
}

impl MessageBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn user() -> Self {
        Self {
            role: Some(MessageRole::User),
            content: Vec::new(),
        }
    }

    pub fn assistant() -> Self {
        Self {
            role: Some(MessageRole::Assistant),
            content: Vec::new(),
        }
    }

    pub fn system() -> Self {
        Self {
            role: Some(MessageRole::System),
            content: Vec::new(),
        }
    }

    pub fn role(mut self, role: MessageRole) -> Self {
        self.role = Some(role);
        self
    }

    pub fn text(mut self, content: impl Into<String>) -> Self {
        self.content.push(ContentBlock::text(content));
        self
    }

    pub fn image_base64(mut self, media_type: impl Into<String>, data: impl Into<String>) -> Self {
        self.content.push(ContentBlock::image_base64(media_type, data));
        self
    }

    pub fn image_url(mut self, url: impl Into<String>) -> Self {
        self.content.push(ContentBlock::image_url(url));
        self
    }

    pub fn tool_use(mut self, id: impl Into<String>, name: impl Into<String>, input: serde_json::Value) -> Self {
        self.content.push(ContentBlock::tool_use(id, name, input));
        self
    }

    pub fn tool_result(mut self, tool_use_id: impl Into<String>, content: impl Into<String>, is_error: bool) -> Self {
        self.content.push(ContentBlock::tool_result(tool_use_id, content, is_error));
        self
    }

    pub fn thinking(mut self, content: impl Into<String>) -> Self {
        self.content.push(ContentBlock::thinking(content));
        self
    }

    pub fn block(mut self, block: ContentBlock) -> Self {
        self.content.push(block);
        self
    }

    pub fn build(self) -> Message {
        Message {
            role: self.role.unwrap_or(MessageRole::User),
            content: self.content,
        }
    }
}
