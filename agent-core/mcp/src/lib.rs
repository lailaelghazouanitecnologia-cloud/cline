#![deny(clippy::all)]
#![forbid(unsafe_code)]

mod client;
mod manager;
mod protocol;
mod transport;
mod types;
mod watcher;

pub use client::McpClient;
pub use manager::*;
pub use protocol::*;
pub use transport::*;
pub use types::*;
pub use watcher::*;
