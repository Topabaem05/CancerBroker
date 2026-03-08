use std::path::PathBuf;
use std::time::SystemTime;

use clap::{Parser, Subcommand};
use color_eyre::eyre::{Result, WrapErr};
use serde::Serialize;

use crate::config::load_config;
use crate::daemon::run_daemon_once;
use crate::policy::SignalWindow;
use crate::runtime::{RuntimeInput, run_once};

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
    RunOnce {
        #[arg(long)]
        json: bool,
        #[arg(long, default_value = ".sisyphus/evidence")]
        evidence_dir: PathBuf,
    },
    Daemon {
        #[arg(long)]
        json: bool,
        #[arg(long, default_value_t = 1)]
        max_events: usize,
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
        Command::RunOnce { json, evidence_dir } => {
            let output = run_once(
                &config,
                RuntimeInput {
                    target_id: "cli-target".to_string(),
                    signal_windows: vec![
                        SignalWindow {
                            name: "rss_slope".to_string(),
                            breached_samples: config.sampling.breach_required_samples,
                            window_samples: config.sampling.breach_window_samples,
                        },
                        SignalWindow {
                            name: "orphan_count".to_string(),
                            breached_samples: config.sampling.breach_required_samples,
                            window_samples: config.sampling.breach_window_samples,
                        },
                    ],
                    history: Vec::new(),
                    now: SystemTime::now(),
                    evidence_dir,
                },
            );

            if json {
                println!("{}", serde_json::to_string(&output)?);
            } else {
                println!(
                    "proposed_action={:?} executed_action={:?}",
                    output.proposed_action, output.executed_action
                );
            }
        }
        Command::Daemon { json, max_events } => {
            let runtime = tokio::runtime::Runtime::new().wrap_err("tokio runtime init failure")?;
            let output = runtime
                .block_on(run_daemon_once(&config, max_events))
                .wrap_err("daemon run failure")?;

            if json {
                println!("{}", serde_json::to_string(&output)?);
            } else {
                println!(
                    "socket_path={} received_events={} processed_events={} reconciled_events={}",
                    output.socket_path.display(),
                    output.received_events,
                    output.processed_events,
                    output.reconciled_events
                );
            }
        }
    }

    Ok(())
}
