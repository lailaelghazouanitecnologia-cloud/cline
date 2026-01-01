#![deny(clippy::all)]
#![forbid(unsafe_code)]

mod anthropic;
mod openai;

pub use anthropic::AnthropicProvider;
pub use openai::OpenAiProvider;
