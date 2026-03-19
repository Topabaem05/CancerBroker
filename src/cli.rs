use std::path::PathBuf;
use std::time::SystemTime;

use clap::{Parser, Subcommand};
use color_eyre::eyre::{Result, WrapErr, eyre};
use serde::Serialize;
use std::time::Duration;

use crate::config::{GuardianConfig, default_notification_session_state_path, load_config};
use crate::daemon::{
    DaemonRunOptions, MemoryGuardOutput, run_daemon_loop, run_rust_analyzer_memory_guard_once,
};
use crate::evidence::default_evidence_dir;
use crate::mcp::run_mcp_server;
use crate::notification_session::refresh_notification_session_snapshot;
use crate::notifications::send_smoke_notification;
use crate::orphans::{OrphanMode, OrphansOutput, run_orphans};
use crate::policy::SignalWindow;
use crate::runtime::{RuntimeInput, RuntimeOutcome, run_once};
use crate::setup::{
    SetupOutcome, default_setup_options, setup as setup_opencode, uninstall as uninstall_opencode,
};

#[derive(Debug, Parser)]
#[command(name = "cancerbroker")]
pub struct Cli {
    #[arg(long)]
    pub config: Option<PathBuf>,
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
        #[arg(long)]
        evidence_dir: Option<PathBuf>,
    },
    Daemon {
        #[arg(long)]
        json: bool,
        #[arg(long, default_value_t = 1)]
        max_events: usize,
    },
    RaGuard {
        #[arg(long)]
        json: bool,
    },
    NotifySmoke {
        #[arg(long)]
        json: bool,
    },
    Orphans {
        #[arg(long)]
        json: bool,
        #[arg(long)]
        dry_run: bool,
        #[arg(long, short = 'k')]
        kill: bool,
        #[arg(long)]
        force: bool,
        #[command(subcommand)]
        action: Option<OrphanAction>,
    },
    Mcp,
    Setup {
        #[arg(long)]
        uninstall: bool,
        #[arg(long)]
        non_interactive: bool,
        #[arg(long)]
        mcp_only: bool,
    },
}

#[derive(Debug, Subcommand)]
pub enum OrphanAction {
    Watch {
        #[arg(long)]
        json: bool,
        #[arg(long, default_value_t = 2)]
        interval_secs: u64,
    },
    Guard {
        #[arg(long)]
        json: bool,
        #[arg(long, default_value_t = 1024)]
        threshold_mb: u64,
        #[arg(long, default_value_t = 60)]
        interval_secs: u64,
        #[arg(long)]
        force: bool,
    },
}

#[derive(Debug, Serialize)]
struct StatusOutput<'a> {
    mode: &'a str,
}

#[derive(Debug, Serialize)]
struct NotifySmokeOutput {
    notified: bool,
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

pub(crate) fn default_signal_windows(config: &crate::config::GuardianConfig) -> Vec<SignalWindow> {
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

fn require_config_path(config: Option<PathBuf>) -> Result<PathBuf> {
    config.ok_or_else(|| eyre!("--config is required for this command"))
}

fn resolve_evidence_dir(evidence_dir: Option<PathBuf>) -> PathBuf {
    evidence_dir.unwrap_or_else(default_evidence_dir)
}

fn load_guardian_config_or_default(config: Option<PathBuf>) -> Result<GuardianConfig> {
    match config {
        Some(path) => load_config(&path).wrap_err("config load failure"),
        None => Ok(GuardianConfig::default()),
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
            "socket_path={} received_events={} processed_events={} reconciled_events={} leak_candidates={} leak_process_remediations={} leak_group_remediations={} rust_analyzer_memory_candidates={} rust_analyzer_memory_remediations={}",
            output.socket_path.display(),
            output.received_events,
            output.processed_events,
            output.reconciled_events,
            output.leak_candidates,
            output.leak_process_remediations,
            output.leak_group_remediations,
            output.rust_analyzer_memory_candidates,
            output.rust_analyzer_memory_remediations
        ))
    }
}

fn render_ra_guard_output(output: &MemoryGuardOutput, json: bool) -> Result<String> {
    if json {
        Ok(serde_json::to_string(output)?)
    } else {
        Ok(format!(
            "rust_analyzer_memory_candidates={} rust_analyzer_memory_remediations={}",
            output.rust_analyzer_memory_candidates, output.rust_analyzer_memory_remediations
        ))
    }
}

fn render_setup_output(output: &SetupOutcome) -> String {
    let mut parts = vec![
        format!("opencode_config={}", output.opencode_config_path.display()),
        format!("installed={}", output.installed),
    ];

    if let Some(backup_path) = &output.opencode_backup_path {
        parts.push(format!("opencode_backup_path={}", backup_path.display()));
    }
    if let Some(config_path) = &output.guardian_config_path {
        parts.push(format!("guardian_config={}", config_path.display()));
    }
    if let Some(backup_path) = &output.guardian_backup_path {
        parts.push(format!("guardian_backup_path={}", backup_path.display()));
    }

    parts.join(" ")
}

fn render_notify_smoke_output(output: &NotifySmokeOutput, json: bool) -> Result<String> {
    if json {
        Ok(serde_json::to_string(output)?)
    } else if output.notified {
        Ok("desktop_notification_triggered".to_string())
    } else {
        Ok("desktop_notification_not_triggered".to_string())
    }
}

fn render_orphans_output(output: &OrphansOutput, json: bool) -> Result<String> {
    if json {
        return Ok(serde_json::to_string(output)?);
    }

    if output.matched_count == 0 {
        return Ok("✅ 깨끗합니다!".to_string());
    }

    let mut lines = vec![format!(
        "mode={} matched_count={} terminated_count={} rejected_count={} estimated_freed_mib={} tty_supported={}",
        output.mode,
        output.matched_count,
        output.terminated_count,
        output.rejected_count,
        output.estimated_freed_bytes / (1024 * 1024),
        output.tty_supported,
    )];

    if let Some(threshold_bytes) = output.threshold_bytes {
        lines.push(format!(
            "threshold_mib={} cycle_index={}",
            threshold_bytes / (1024 * 1024),
            output.cycle_index.unwrap_or(0)
        ));
    }

    for process in &output.processes {
        lines.push(format!(
            "pid={} memory_mib={} cpu_percent={:.1} tty={} command={}",
            process.pid,
            process.memory_bytes / (1024 * 1024),
            process.cpu_percent_milli as f32 / 1000.0,
            process.tty.as_deref().unwrap_or("<none>"),
            process.command,
        ));
    }

    Ok(lines.join("\n"))
}

pub fn run(cli: Cli) -> Result<()> {
    let _ = refresh_notification_session_snapshot(&default_notification_session_state_path());

    match cli.command {
        Command::Status { json } => {
            let config_path = require_config_path(cli.config)?;
            let config = load_config(&config_path).wrap_err("config load failure")?;
            let _ = refresh_notification_session_snapshot(&config.notifications.session_state_path);
            let output = build_status_output(&config);
            println!("{}", render_status_output(&output, json)?);
        }
        Command::RunOnce { json, evidence_dir } => {
            let config_path = require_config_path(cli.config)?;
            let config = load_config(&config_path).wrap_err("config load failure")?;
            let _ = refresh_notification_session_snapshot(&config.notifications.session_state_path);
            let output = run_once(
                &config,
                build_runtime_input(
                    &config,
                    resolve_evidence_dir(evidence_dir),
                    SystemTime::now(),
                ),
            );
            println!("{}", render_runtime_output(&output, json)?);
        }
        Command::Daemon { json, max_events } => {
            let config_path = require_config_path(cli.config)?;
            let config = load_config(&config_path).wrap_err("config load failure")?;
            let _ = refresh_notification_session_snapshot(&config.notifications.session_state_path);
            let runtime = tokio::runtime::Runtime::new().wrap_err("tokio runtime init failure")?;
            let output = runtime
                .block_on(run_daemon_loop(
                    &config,
                    build_daemon_run_options(&config, max_events),
                ))
                .wrap_err("daemon run failure")?;
            println!("{}", render_daemon_output(&output, json)?);
        }
        Command::RaGuard { json } => {
            let config_path = require_config_path(cli.config)?;
            let config = load_config(&config_path).wrap_err("config load failure")?;
            let _ = refresh_notification_session_snapshot(&config.notifications.session_state_path);
            let output = run_rust_analyzer_memory_guard_once(&config)
                .wrap_err("rust-analyzer guard run failure")?;
            println!("{}", render_ra_guard_output(&output, json)?);
        }
        Command::NotifySmoke { json } => {
            let default_path = default_notification_session_state_path();
            let _ = refresh_notification_session_snapshot(&default_path);
            send_smoke_notification(Some(default_path.as_path())).map_err(|error| eyre!(error))?;
            let output = NotifySmokeOutput { notified: true };
            println!("{}", render_notify_smoke_output(&output, json)?);
        }
        Command::Orphans {
            json,
            dry_run: _,
            kill,
            force,
            action,
        } => {
            let config = load_guardian_config_or_default(cli.config)?;
            let outputs = match action {
                Some(OrphanAction::Watch { interval_secs, .. }) => run_orphans(
                    &config,
                    OrphanMode::Watch {
                        interval: Duration::from_secs(interval_secs.max(1)),
                        max_cycles: None,
                    },
                ),
                Some(OrphanAction::Guard {
                    threshold_mb,
                    interval_secs,
                    force,
                    ..
                }) => run_orphans(
                    &config,
                    OrphanMode::Guard {
                        threshold_bytes: threshold_mb.saturating_mul(1024 * 1024),
                        interval: Duration::from_secs(interval_secs.max(1)),
                        max_cycles: None,
                        force,
                    },
                ),
                None if kill => run_orphans(&config, OrphanMode::Kill { force }),
                None => run_orphans(&config, OrphanMode::List),
            }
            .map_err(|error| eyre!(error))?;

            let effective_json = match &action {
                Some(OrphanAction::Watch { json, .. }) => *json,
                Some(OrphanAction::Guard { json, .. }) => *json,
                None => json,
            };

            for output in outputs {
                println!("{}", render_orphans_output(&output, effective_json)?);
            }
        }
        Command::Mcp => {
            let runtime = tokio::runtime::Runtime::new().wrap_err("tokio runtime init failure")?;
            runtime
                .block_on(run_mcp_server(cli.config))
                .wrap_err("mcp run failure")?;
        }
        Command::Setup {
            uninstall,
            non_interactive,
            mcp_only,
        } => {
            let output = if uninstall {
                uninstall_opencode()?
            } else {
                let mut options = default_setup_options(non_interactive);
                options.mcp_only = mcp_only;
                setup_opencode(options)?
            };
            println!("{}", render_setup_output(&output));
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
        Cli, Command, NotifySmokeOutput, OrphanAction, build_daemon_run_options,
        build_runtime_input, build_status_output, default_signal_windows, render_daemon_output,
        render_notify_smoke_output, render_orphans_output, render_ra_guard_output,
        render_runtime_output, render_setup_output, render_status_output, require_config_path,
        resolve_evidence_dir,
    };
    use crate::config::{CompletionCleanupPolicy, GuardianConfig, Mode, SamplingPolicy};
    use crate::daemon::{DaemonOutput, MemoryGuardOutput};
    use crate::orphans::OrphansOutput;
    use crate::runtime::RuntimeOutcome;
    use crate::setup::SetupOutcome;

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
            leak_candidates: 4,
            leak_process_remediations: 1,
            leak_group_remediations: 1,
            rust_analyzer_memory_candidates: 2,
            rust_analyzer_memory_remediations: 1,
        };

        assert_eq!(
            render_daemon_output(&output, false).expect("human daemon output"),
            "socket_path=/tmp/guardian.sock received_events=3 processed_events=2 reconciled_events=1 leak_candidates=4 leak_process_remediations=1 leak_group_remediations=1 rust_analyzer_memory_candidates=2 rust_analyzer_memory_remediations=1"
        );
        assert!(
            render_daemon_output(&output, true)
                .expect("json daemon output")
                .contains("\"rust_analyzer_memory_candidates\":2")
        );
    }

    #[test]
    fn ra_guard_output_renders_human_and_json_modes() {
        let output = MemoryGuardOutput {
            rust_analyzer_memory_candidates: 3,
            rust_analyzer_memory_remediations: 1,
        };

        assert_eq!(
            render_ra_guard_output(&output, false).expect("human ra-guard output"),
            "rust_analyzer_memory_candidates=3 rust_analyzer_memory_remediations=1"
        );
        assert_eq!(
            render_ra_guard_output(&output, true).expect("json ra-guard output"),
            r#"{"rust_analyzer_memory_candidates":3,"rust_analyzer_memory_remediations":1}"#
        );
    }

    #[test]
    fn require_config_path_rejects_missing_value() {
        let error = require_config_path(None).expect_err("missing config should fail");

        assert!(error.to_string().contains("--config is required"));
    }

    #[test]
    fn resolve_evidence_dir_uses_default_when_missing() {
        let resolved = resolve_evidence_dir(None);

        assert!(!resolved.as_os_str().is_empty());
    }

    #[test]
    fn setup_output_renders_backup_when_present() {
        let output = render_setup_output(&SetupOutcome {
            opencode_config_path: PathBuf::from("/tmp/opencode.json"),
            opencode_backup_path: Some(PathBuf::from("/tmp/opencode.json.bak")),
            guardian_config_path: Some(PathBuf::from("/tmp/guardian.toml")),
            guardian_backup_path: Some(PathBuf::from("/tmp/guardian.toml.bak")),
            installed: true,
        });

        assert!(output.contains("opencode_config=/tmp/opencode.json"));
        assert!(output.contains("opencode_backup_path=/tmp/opencode.json.bak"));
        assert!(output.contains("guardian_config=/tmp/guardian.toml"));
        assert!(output.contains("guardian_backup_path=/tmp/guardian.toml.bak"));
        assert!(output.contains("installed=true"));
    }

    #[test]
    fn notify_smoke_output_renders_human_and_json_modes() {
        let output = NotifySmokeOutput { notified: true };

        assert_eq!(
            render_notify_smoke_output(&output, false).expect("human notify output"),
            "desktop_notification_triggered"
        );
        assert_eq!(
            render_notify_smoke_output(&output, true).expect("json notify output"),
            r#"{"notified":true}"#
        );
    }

    #[test]
    fn orphan_output_renders_clean_human_message_and_json() {
        let output = OrphansOutput {
            mode: "list".to_string(),
            tty_supported: true,
            matched_count: 0,
            terminated_count: 0,
            already_exited_count: 0,
            rejected_count: 0,
            estimated_freed_bytes: 0,
            threshold_bytes: None,
            cycle_index: None,
            processes: Vec::new(),
        };

        assert_eq!(
            render_orphans_output(&output, false).expect("human orphan output"),
            "✅ 깨끗합니다!"
        );
        assert!(
            render_orphans_output(&output, true)
                .expect("json orphan output")
                .contains("\"matched_count\":0")
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
            Some(PathBuf::from("fixtures/config/observe-only.toml"))
        );
        match cli.command {
            Command::RunOnce { json, evidence_dir } => {
                assert!(json);
                assert_eq!(evidence_dir, Some(PathBuf::from("/tmp/evidence")));
            }
            _ => panic!("expected run-once command"),
        }
    }

    #[test]
    fn clap_parser_builds_setup_command_without_config() {
        let cli = Cli::parse_from(["cancerbroker", "setup", "--uninstall", "--non-interactive"]);

        assert_eq!(cli.config, None);
        match cli.command {
            Command::Setup {
                uninstall,
                non_interactive,
                mcp_only,
            } => {
                assert!(uninstall);
                assert!(non_interactive);
                assert!(!mcp_only);
            }
            _ => panic!("expected setup command"),
        }
    }

    #[test]
    fn clap_parser_builds_setup_command_with_mcp_only_flag() {
        let cli = Cli::parse_from(["cancerbroker", "setup", "--mcp-only"]);

        assert_eq!(cli.config, None);
        match cli.command {
            Command::Setup {
                uninstall,
                non_interactive,
                mcp_only,
            } => {
                assert!(!uninstall);
                assert!(!non_interactive);
                assert!(mcp_only);
            }
            _ => panic!("expected setup command"),
        }
    }

    #[test]
    fn clap_parser_builds_mcp_command_with_optional_config() {
        let cli = Cli::parse_from(["cancerbroker", "--config", "/tmp/guardian.toml", "mcp"]);

        assert_eq!(cli.config, Some(PathBuf::from("/tmp/guardian.toml")));
        assert!(matches!(cli.command, Command::Mcp));
    }

    #[test]
    fn clap_parser_builds_ra_guard_command() {
        let cli = Cli::parse_from([
            "cancerbroker",
            "--config",
            "/tmp/guardian.toml",
            "ra-guard",
            "--json",
        ]);

        assert_eq!(cli.config, Some(PathBuf::from("/tmp/guardian.toml")));
        match cli.command {
            Command::RaGuard { json } => assert!(json),
            _ => panic!("expected ra-guard command"),
        }
    }

    #[test]
    fn clap_parser_builds_notify_smoke_command() {
        let cli = Cli::parse_from(["cancerbroker", "notify-smoke", "--json"]);

        assert_eq!(cli.config, None);
        match cli.command {
            Command::NotifySmoke { json } => assert!(json),
            _ => panic!("expected notify-smoke command"),
        }
    }

    #[test]
    fn clap_parser_builds_orphans_list_and_watch_commands() {
        let list_cli = Cli::parse_from(["cancerbroker", "orphans", "--json"]);
        match list_cli.command {
            Command::Orphans {
                json,
                dry_run,
                kill,
                action,
                ..
            } => {
                assert!(json);
                assert!(!dry_run);
                assert!(!kill);
                assert!(action.is_none());
            }
            _ => panic!("expected orphans command"),
        }

        let watch_cli = Cli::parse_from([
            "cancerbroker",
            "orphans",
            "watch",
            "--json",
            "--interval-secs",
            "5",
        ]);
        match watch_cli.command {
            Command::Orphans {
                action:
                    Some(OrphanAction::Watch {
                        json,
                        interval_secs,
                    }),
                ..
            } => {
                assert!(json);
                assert_eq!(interval_secs, 5);
            }
            _ => panic!("expected orphans watch command"),
        }
    }

    #[test]
    fn clap_parser_rejects_ra_guard_max_events_flag() {
        let error = Cli::try_parse_from([
            "cancerbroker",
            "--config",
            "/tmp/guardian.toml",
            "ra-guard",
            "--max-events",
            "1",
        ])
        .expect_err("ra-guard should not accept --max-events");

        let message = error.to_string();
        assert!(message.contains("--max-events"));
        assert!(message.contains("ra-guard"));
    }
}
