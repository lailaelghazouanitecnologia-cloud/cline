use agent_common::SubmissionId;
use serde::{Deserialize, Serialize};

use crate::operation::Operation;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Submission {
    pub id: SubmissionId,
    pub operation: Operation,
}

impl Submission {
    pub fn new(id: SubmissionId, operation: Operation) -> Self {
        Self { id, operation }
    }
}
