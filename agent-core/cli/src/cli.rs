#![deny(clippy::all)]

use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "cline-agent")]
#[command(about = "AI coding agent powered by LLMs")]
pub struct Args {
    #[arg(short, long, default_value = "groq")]
    pub provider: String,

    #[arg(short, long)]
    pub model: Option<String>,

    #[arg(short, long, env = "GROQ_API_KEY")]
    pub api_key: Option<String>,

    #[arg(long)]
    pub working_dir: Option<PathBuf>,

    #[arg(long, default_value = "false")]
    pub yolo: bool,

    #[arg(long, default_value = "false")]
    pub auto_edit: bool,

    #[arg(long, default_value = "false")]
    pub sandbox: bool,

    #[arg(long, default_value = "50")]
    pub max_turns: u32,

    #[arg(short, long)]
    pub task: Option<String>,

    #[arg(long, default_value = "false")]
    pub verbose: bool,

    #[arg(long, default_value = "false")]
    pub serve: bool,

    #[arg(long, default_value = "3001")]
    pub port: u16,
}

impl Args {
    pub fn api_key(&self) -> Option<String> {
        self.api_key.clone().or_else(|| {
            let env_var = match self.provider.as_str() {
                "groq" => "GROQ_API_KEY",
                "openai" => "OPENAI_API_KEY",
                "anthropic" => "ANTHROPIC_API_KEY",
                _ => return None,
            };
            std::env::var(env_var).ok()
        })
    }

    pub fn working_directory(&self) -> PathBuf {
        self.working_dir
            .clone()
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default())
    }
}
