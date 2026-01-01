use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Operation {
    UserMessage(UserMessageOperation),
    Approve(ApproveOperation),
    Reject(RejectOperation),
    Interrupt,
    Shutdown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserMessageOperation {
    pub content: String,
    pub images: Vec<PathBuf>,
    pub working_directory: Option<PathBuf>,
}

impl UserMessageOperation {
    pub fn text(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            images: Vec::new(),
            working_directory: None,
        }
    }

    pub fn with_images(mut self, images: Vec<PathBuf>) -> Self {
        self.images = images;
        self
    }

    pub fn with_working_directory(mut self, directory: PathBuf) -> Self {
        self.working_directory = Some(directory);
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApproveOperation {
    pub tool_call_id: String,
    pub feedback: Option<String>,
}

impl ApproveOperation {
    pub fn new(tool_call_id: impl Into<String>) -> Self {
        Self {
            tool_call_id: tool_call_id.into(),
            feedback: None,
        }
    }

    pub fn with_feedback(mut self, feedback: impl Into<String>) -> Self {
        self.feedback = Some(feedback.into());
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RejectOperation {
    pub tool_call_id: String,
    pub reason: Option<String>,
}

impl RejectOperation {
    pub fn new(tool_call_id: impl Into<String>) -> Self {
        Self {
            tool_call_id: tool_call_id.into(),
            reason: None,
        }
    }

    pub fn with_reason(mut self, reason: impl Into<String>) -> Self {
        self.reason = Some(reason.into());
        self
    }
}
