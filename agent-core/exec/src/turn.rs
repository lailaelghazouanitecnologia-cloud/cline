use agent_common::TurnId;
use serde::{Deserialize, Serialize};
use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TurnState {
    Pending,
    Processing,
    WaitingApproval,
    Completed,
    Failed,
    Interrupted,
}

impl TurnState {
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Interrupted)
    }
}

impl Default for TurnState {
    fn default() -> Self {
        Self::Pending
    }
}

pub struct Turn {
    id: TurnId,
    state: TurnState,
    tool_calls_count: u32,
    start_time: Instant,
}

impl Turn {
    pub fn new(id: TurnId) -> Self {
        Self {
            id,
            state: TurnState::Pending,
            tool_calls_count: 0,
            start_time: Instant::now(),
        }
    }

    pub fn id(&self) -> TurnId {
        self.id
    }

    pub fn state(&self) -> TurnState {
        self.state
    }

    pub fn tool_calls_count(&self) -> u32 {
        self.tool_calls_count
    }

    pub fn duration_ms(&self) -> u64 {
        self.start_time.elapsed().as_millis() as u64
    }

    pub fn set_state(&mut self, state: TurnState) {
        self.state = state;
    }

    pub fn increment_tool_calls(&mut self) {
        self.tool_calls_count += 1;
    }
}
