use std::path::PathBuf;
use std::time::SystemTime;

use clap::{Parser, Subcommand};
use color_eyre::eyre::{Result, WrapErr};
use serde::Serialize;
use std::time::Duration;

use crate::config::load_config;
use crate::daemon::{DaemonRunOptions, run_daemon_loop};
use crate::policy::SignalWindow;
use crate::runtime::{RuntimeInput, RuntimeOutcome, run_once};

#[derive(Debug, Parser)]
#[command(name = "cancerbroker")]
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

fn build_status_output(config: &crate::config::GuardianConfig) -> StatusOutput<'_> {
    StatusOutput {
        mode: config.mode.as_str(),
    }
}

fn render_status_output(output: &StatusOutput<'_>, json: bool) -> Result<String> {
    if json {
        Ok(serde_json::to_string(output)?)
    } else {
        Ok(format!("mode={}", output.mode))
    }
}

fn default_signal_windows(config: &crate::config::GuardianConfig) -> Vec<SignalWindow> {
    vec![
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
    ]
}

fn build_runtime_input(
    config: &crate::config::GuardianConfig,
    evidence_dir: PathBuf,
    now: SystemTime,
) -> RuntimeInput {
    RuntimeInput {
        target_id: "cli-target".to_string(),
        signal_windows: default_signal_windows(config),
        history: Vec::new(),
        now,
        evidence_dir,
    }
}

fn render_runtime_output(output: &RuntimeOutcome, json: bool) -> Result<String> {
    if json {
        Ok(serde_json::to_string(output)?)
    } else {
        Ok(format!(
            "proposed_action={:?} executed_action={:?}",
            output.proposed_action, output.executed_action
        ))
    }
}

fn build_daemon_run_options(
    config: &crate::config::GuardianConfig,
    max_events: usize,
) -> DaemonRunOptions {
    DaemonRunOptions {
        max_events_per_batch: max_events,
        max_cycles: None,
        idle_timeout: Duration::from_secs(config.completion.reconciliation_interval_secs.max(1)),
    }
}

fn render_daemon_output(output: &crate::daemon::DaemonOutput, json: bool) -> Result<String> {
    if json {
        Ok(serde_json::to_string(output)?)
    } else {
        Ok(format!(
            "socket_path={} received_events={} processed_events={} reconciled_events={}",
            output.socket_path.display(),
            output.received_events,
            output.processed_events,
            output.reconciled_events
        ))
    }
}

pub fn run(cli: Cli) -> Result<()> {
    let config = load_config(&cli.config).wrap_err("config load failure")?;

    match cli.command {
        Command::Status { json } => {
            let output = build_status_output(&config);
            println!("{}", render_status_output(&output, json)?);
        }
        Command::RunOnce { json, evidence_dir } => {
            let output = run_once(
                &config,
                build_runtime_input(&config, evidence_dir, SystemTime::now()),
            );
            println!("{}", render_runtime_output(&output, json)?);
        }
        Command::Daemon { json, max_events } => {
            let runtime = tokio::runtime::Runtime::new().wrap_err("tokio runtime init failure")?;
            let output = runtime
                .block_on(run_daemon_loop(
                    &config,
                    build_daemon_run_options(&config, max_events),
                ))
                .wrap_err("daemon run failure")?;
            println!("{}", render_daemon_output(&output, json)?);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::{Duration, UNIX_EPOCH};

    use clap::Parser;

    use super::{
        Cli, Command, build_daemon_run_options, build_runtime_input, build_status_output,
        default_signal_windows, render_daemon_output, render_runtime_output, render_status_output,
    };
    use crate::config::{CompletionCleanupPolicy, GuardianConfig, Mode, SamplingPolicy};
    use crate::daemon::DaemonOutput;
    use crate::runtime::RuntimeOutcome;

    #[test]
    fn status_output_renders_human_and_json_modes() {
        let config = GuardianConfig {
            mode: Mode::Enforce,
            ..GuardianConfig::default()
        };
        let output = build_status_output(&config);

        assert_eq!(
            render_status_output(&output, false).expect("human output"),
            "mode=enforce"
        );
        assert_eq!(
            render_status_output(&output, true).expect("json output"),
            r#"{"mode":"enforce"}"#
        );
    }

    #[test]
    fn default_signal_windows_follow_sampling_thresholds() {
        let config = GuardianConfig {
            sampling: SamplingPolicy {
                breach_required_samples: 4,
                breach_window_samples: 7,
                ..SamplingPolicy::default()
            },
            ..GuardianConfig::default()
        };

        let windows = default_signal_windows(&config);

        assert_eq!(windows.len(), 2);
        assert_eq!(windows[0].name, "rss_slope");
        assert_eq!(windows[1].name, "orphan_count");
        assert_eq!(windows[0].breached_samples, 4);
        assert_eq!(windows[1].window_samples, 7);
    }

    #[test]
    fn runtime_input_uses_cli_target_and_evidence_dir() {
        let config = GuardianConfig::default();
        let evidence_dir = PathBuf::from("/tmp/evidence");

        let input = build_runtime_input(&config, evidence_dir.clone(), UNIX_EPOCH);

        assert_eq!(input.target_id, "cli-target");
        assert_eq!(input.evidence_dir, evidence_dir);
        assert_eq!(input.now, UNIX_EPOCH);
        assert!(input.history.is_empty());
        assert_eq!(input.signal_windows.len(), 2);
    }

    #[test]
    fn runtime_output_renders_human_and_json_modes() {
        let output = RuntimeOutcome {
            proposed_action: Some("terminate".to_string()),
            executed_action: None,
            evidence_path: Some(PathBuf::from("/tmp/evidence.json")),
            fallback_to_non_destructive: true,
        };

        assert_eq!(
            render_runtime_output(&output, false).expect("human runtime output"),
            "proposed_action=Some(\"terminate\") executed_action=None"
        );
        assert!(
            render_runtime_output(&output, true)
                .expect("json runtime output")
                .contains("\"fallback_to_non_destructive\":true")
        );
    }

    #[test]
    fn daemon_run_options_use_minimum_one_second_idle_timeout() {
        let config = GuardianConfig {
            completion: CompletionCleanupPolicy {
                reconciliation_interval_secs: 0,
                ..CompletionCleanupPolicy::default()
            },
            ..GuardianConfig::default()
        };

        let options = build_daemon_run_options(&config, 8);

        assert_eq!(options.max_events_per_batch, 8);
        assert_eq!(options.max_cycles, None);
        assert_eq!(options.idle_timeout, Duration::from_secs(1));
    }

    #[test]
    fn daemon_output_renders_human_and_json_modes() {
        let output = DaemonOutput {
            socket_path: PathBuf::from("/tmp/guardian.sock"),
            received_events: 3,
            processed_events: 2,
            reconciled_events: 1,
        };

        assert_eq!(
            render_daemon_output(&output, false).expect("human daemon output"),
            "socket_path=/tmp/guardian.sock received_events=3 processed_events=2 reconciled_events=1"
        );
        assert!(
            render_daemon_output(&output, true)
                .expect("json daemon output")
                .contains("\"processed_events\":2")
        );
    }

    #[test]
    fn clap_parser_builds_run_once_command() {
        let cli = Cli::parse_from([
            "cancerbroker",
            "--config",
            "fixtures/config/observe-only.toml",
            "run-once",
            "--json",
            "--evidence-dir",
            "/tmp/evidence",
        ]);

        assert_eq!(
            cli.config,
            PathBuf::from("fixtures/config/observe-only.toml")
        );
        match cli.command {
            Command::RunOnce { json, evidence_dir } => {
                assert!(json);
                assert_eq!(evidence_dir, PathBuf::from("/tmp/evidence"));
            }
            _ => panic!("expected run-once command"),
        }
    }
}
