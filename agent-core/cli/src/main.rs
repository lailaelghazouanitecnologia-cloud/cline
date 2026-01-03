#![deny(clippy::all)]

mod agent_runner;
mod cli;
mod repl;
mod tool_bridge;

use clap::Parser;
use cli::Args;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    setup_tracing();

    let args = Args::parse();

    if let Err(e) = run(args).await {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}

async fn run(args: Args) -> agent_common::AgentResult<()> {
    let runner = agent_runner::AgentRunner::new(args)?;
    runner.run().await
}

fn setup_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();
}
