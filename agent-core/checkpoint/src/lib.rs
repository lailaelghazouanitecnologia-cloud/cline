#![deny(clippy::all)]
#![forbid(unsafe_code)]

mod diff;
mod manager;
mod recovery;
mod snapshot;

pub use diff::{DiffHunk, FileDiff};
pub use manager::CheckpointManager;
pub use recovery::{IncrementalBackup, RecoveryManager, RecoveryResult, RecoveryState, RecoveryStrategy};
pub use snapshot::{Checkpoint, CheckpointMeta};
