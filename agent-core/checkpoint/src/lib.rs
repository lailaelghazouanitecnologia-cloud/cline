#![deny(clippy::all)]
#![forbid(unsafe_code)]

mod manager;
mod snapshot;
mod diff;

pub use manager::CheckpointManager;
pub use snapshot::{Checkpoint, CheckpointMeta};
pub use diff::{FileDiff, DiffHunk};
