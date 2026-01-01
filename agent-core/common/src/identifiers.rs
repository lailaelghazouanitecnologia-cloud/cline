use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SubmissionId(u64);

impl SubmissionId {
    pub fn new(value: u64) -> Self {
        debug_assert!(value > 0);
        Self(value)
    }

    pub fn value(&self) -> u64 {
        self.0
    }
}

impl std::fmt::Display for SubmissionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "sub_{}", self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EventId(u64);

impl EventId {
    pub fn new(value: u64) -> Self {
        debug_assert!(value > 0);
        Self(value)
    }

    pub fn value(&self) -> u64 {
        self.0
    }
}

impl std::fmt::Display for EventId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "evt_{}", self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TurnId(u64);

impl TurnId {
    pub fn new(value: u64) -> Self {
        Self(value)
    }

    pub fn value(&self) -> u64 {
        self.0
    }
}

impl std::fmt::Display for TurnId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "turn_{}", self.0)
    }
}

pub struct IdGenerator {
    counter: AtomicU64,
}

impl IdGenerator {
    pub const fn new() -> Self {
        Self {
            counter: AtomicU64::new(0),
        }
    }

    pub fn next_submission(&self) -> SubmissionId {
        let value = self.counter.fetch_add(1, Ordering::SeqCst) + 1;
        SubmissionId::new(value)
    }

    pub fn next_event(&self) -> EventId {
        let value = self.counter.fetch_add(1, Ordering::SeqCst) + 1;
        EventId::new(value)
    }

    pub fn next_turn(&self) -> TurnId {
        let value = self.counter.fetch_add(1, Ordering::SeqCst) + 1;
        TurnId::new(value)
    }
}

impl Default for IdGenerator {
    fn default() -> Self {
        Self::new()
    }
}
