#![deny(clippy::all)]

mod agent_runner;
mod approval;
mod cli;
mod context;
mod repl;
mod server;
mod slash_commands;
mod store;
mod tool_bridge;
mod usage;

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
    let locations = build_env_locations();
    for loc in locations.into_iter().flatten() {
        if loc.exists() {
            if dotenvy::from_path(&loc).is_ok() {
                eprintln!("[env] Loaded from: {}", loc.display());
                return;
            }
        }
    }
    let _ = dotenvy::dotenv();
}

fn build_env_locations() -> [Option<std::path::PathBuf>; 6] {
    let cwd = std::env::current_dir().ok();
    let exe = std::env::current_exe().ok();
    let home = dirs::home_dir();

    [
        cwd.clone().map(|p| p.join(".env")),
        cwd.clone().and_then(|p| p.parent().map(|pp| pp.join(".env"))),
        cwd.and_then(|p| p.parent().and_then(|pp| pp.parent().map(|ppp| ppp.join("agent-core/.env")))),
        exe.clone().and_then(|p| p.parent().map(|pp| pp.join(".env"))),
        exe.and_then(|p| p.ancestors().nth(3).map(|pp| pp.join("agent-core/.env"))),
        home.map(|p| p.join(".config/cline-agent/.env")),
    ]
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
