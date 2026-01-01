#![deny(clippy::all)]
#![forbid(unsafe_code)]

mod error;
mod identifiers;
mod limits;

pub use error::AgentError;
pub use identifiers::*;
pub use limits::*;

pub type AgentResult<T> = Result<T, AgentError>;
