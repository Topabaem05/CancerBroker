use std::path::PathBuf;

use clap::{Parser, Subcommand};
use color_eyre::eyre::{Result, WrapErr};
use serde::Serialize;

use crate::config::load_config;

#[derive(Debug, Parser)]
#[command(name = "opencode-guardian")]
pub struct Cli {
    #[arg(long)]
    pub config: PathBuf,
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    Status {
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Serialize)]
struct StatusOutput<'a> {
    mode: &'a str,
}

pub fn run(cli: Cli) -> Result<()> {
    let config = load_config(&cli.config).wrap_err("config load failure")?;

    match cli.command {
        Command::Status { json } => {
            let output = StatusOutput {
                mode: config.mode.as_str(),
            };
            if json {
                println!("{}", serde_json::to_string(&output)?);
            } else {
                println!("mode={}", output.mode);
            }
        }
    }

    Ok(())
}
