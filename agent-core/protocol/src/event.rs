use agent_common::EventId;
use serde::{Deserialize, Serialize};

use crate::message::EventMessage;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub id: EventId,
    pub submission_id: Option<String>,
    pub message: EventMessage,
}

impl Event {
    pub fn new(id: EventId, message: EventMessage) -> Self {
        Self {
            id,
            submission_id: None,
            message,
        }
    }

    pub fn with_submission(mut self, submission_id: String) -> Self {
        self.submission_id = Some(submission_id);
        self
    }
}
