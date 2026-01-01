#![deny(clippy::all)]
#![forbid(unsafe_code)]

mod chain;
mod item;
mod watcher;

pub use chain::{FocusChain, FocusEvent};
pub use item::{FocusItem, FocusStatus, FocusType};
pub use watcher::FocusWatcher;
