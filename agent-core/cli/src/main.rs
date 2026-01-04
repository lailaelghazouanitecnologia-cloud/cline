#![deny(clippy::all)]

mod agent_runner;
mod approval;
mod cli;
mod repl;
mod server;
mod store;
mod tool_bridge;

use clap::Parser;
use cli::Args;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    load_env();
    setup_tracing();

    let args = Args::parse();

    if let Err(e) = run(args).await {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}

fn load_env() {
    if dotenvy::dotenv().is_ok() {
        return;
    }
    let locations = [
        std::env::current_dir().ok().map(|p| p.join(".env")),
        std::env::current_dir().ok().and_then(|p| p.parent().map(|pp| pp.join(".env"))),
        std::env::current_exe().ok().and_then(|p| p.parent().map(|pp| pp.join(".env"))),
        dirs::home_dir().map(|p| p.join(".config/cline-agent/.env")),
    ];
    for loc in locations.into_iter().flatten() {
        if loc.exists() && dotenvy::from_path(&loc).is_ok() {
            return;
        }
    }
}

async fn run(args: Args) -> agent_common::AgentResult<()> {
    if args.serve {
        server::run_server(args).await
    } else {
        let runner = agent_runner::AgentRunner::new(args)?;
        runner.run().await
    }
}

fn setup_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();
}
