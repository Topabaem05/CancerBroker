use std::collections::HashMap;
use std::thread;
use std::time::Duration;

#[cfg(unix)]
use std::process::Command;

use serde::Serialize;
use thiserror::Error;

use crate::config::GuardianConfig;
use crate::notifications::{NotificationContext, RemediationReason, notify_process_terminated};
use crate::platform::current_effective_uid;
use crate::remediation::{ProcessRemediationOutcome, remediate_process, remediate_process_force};
use crate::safety::{OwnershipPolicy, ProcessIdentity, SafetyDecision, validate_process_identity};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OrphanProcessOutput {
    pub pid: u32,
    pub parent_pid: Option<u32>,
    pub pgid: Option<u32>,
    pub memory_bytes: u64,
    pub cpu_percent_milli: u32,
    pub tty: Option<String>,
    pub command: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OrphansOutput {
    pub mode: String,
    pub tty_supported: bool,
    pub matched_count: usize,
    pub terminated_count: usize,
    pub already_exited_count: usize,
    pub rejected_count: usize,
    pub estimated_freed_bytes: u64,
    pub threshold_bytes: Option<u64>,
    pub cycle_index: Option<usize>,
    pub processes: Vec<OrphanProcessOutput>,
}

#[derive(Debug, Clone)]
pub enum OrphanMode {
    List,
    Kill {
        force: bool,
    },
    Watch {
        interval: Duration,
        max_cycles: Option<usize>,
    },
    Guard {
        threshold_bytes: u64,
        interval: Duration,
        max_cycles: Option<usize>,
        force: bool,
    },
}

#[derive(Debug, Error)]
pub enum OrphanError {
    #[error("process collection failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("remediation failed: {0}")]
    Remediation(#[from] crate::remediation::RemediationError),
}

#[derive(Debug, Clone)]
struct OrphanCandidate {
    identity: ProcessIdentity,
    cpu_percent: f32,
    tty: Option<String>,
    command: String,
}

pub fn run_orphans(
    config: &GuardianConfig,
    mode: OrphanMode,
) -> Result<Vec<OrphansOutput>, OrphanError> {
    match mode {
        OrphanMode::List => Ok(vec![scan_once(config, "list", None)?]),
        OrphanMode::Kill { force } => Ok(vec![kill_once(config, force)?]),
        OrphanMode::Watch {
            interval,
            max_cycles,
        } => run_watch(config, interval, max_cycles),
        OrphanMode::Guard {
            threshold_bytes,
            interval,
            max_cycles,
            force,
        } => run_guard(config, threshold_bytes, interval, max_cycles, force),
    }
}

fn run_watch(
    config: &GuardianConfig,
    interval: Duration,
    max_cycles: Option<usize>,
) -> Result<Vec<OrphansOutput>, OrphanError> {
    let cycles = max_cycles.unwrap_or(usize::MAX);
    let mut outputs = Vec::new();

    for cycle in 0..cycles {
        let mut output = scan_once(config, "watch", None)?;
        output.cycle_index = Some(cycle);
        outputs.push(output);
        if cycle + 1 < cycles {
            thread::sleep(interval);
        }
    }

    Ok(outputs)
}

fn run_guard(
    config: &GuardianConfig,
    threshold_bytes: u64,
    interval: Duration,
    max_cycles: Option<usize>,
    force: bool,
) -> Result<Vec<OrphansOutput>, OrphanError> {
    let cycles = max_cycles.unwrap_or(usize::MAX);
    let mut outputs = Vec::new();

    for cycle in 0..cycles {
        let mut output = guard_once(config, threshold_bytes, force)?;
        output.cycle_index = Some(cycle);
        outputs.push(output);
        if cycle + 1 < cycles {
            thread::sleep(interval);
        }
    }

    Ok(outputs)
}

fn scan_once(
    config: &GuardianConfig,
    mode: &str,
    threshold_bytes: Option<u64>,
) -> Result<OrphansOutput, OrphanError> {
    let scan = collect_orphan_candidates(config)?;
    Ok(render_output(
        mode,
        threshold_bytes,
        scan.tty_supported,
        scan.candidates,
    ))
}

fn kill_once(config: &GuardianConfig, force: bool) -> Result<OrphansOutput, OrphanError> {
    let scan = collect_orphan_candidates(config)?;
    remediate_candidates(
        config,
        "kill",
        None,
        scan.tty_supported,
        scan.candidates,
        force,
    )
}

fn guard_once(
    config: &GuardianConfig,
    threshold_bytes: u64,
    force: bool,
) -> Result<OrphansOutput, OrphanError> {
    let scan = collect_orphan_candidates(config)?;
    let candidates = scan
        .candidates
        .into_iter()
        .filter(|candidate| candidate.identity.current_rss_bytes > threshold_bytes)
        .collect();
    remediate_candidates(
        config,
        "guard",
        Some(threshold_bytes),
        scan.tty_supported,
        candidates,
        force,
    )
}

fn remediate_candidates(
    config: &GuardianConfig,
    mode: &str,
    threshold_bytes: Option<u64>,
    tty_supported: bool,
    candidates: Vec<OrphanCandidate>,
    force: bool,
) -> Result<OrphansOutput, OrphanError> {
    let ownership_policy = build_orphan_ownership_policy(config);
    let mut terminated_count = 0;
    let mut already_exited_count = 0;
    let mut rejected_count = 0;
    let mut estimated_freed_bytes = 0_u64;
    let mut outputs = Vec::with_capacity(candidates.len());

    for candidate in candidates {
        let request = crate::remediation::ProcessRemediationRequest {
            identity: candidate.identity.clone(),
            ownership_policy: ownership_policy.clone(),
            term_timeout: Duration::from_secs(1),
        };
        let outcome = if force {
            remediate_process_force(&request)?
        } else {
            remediate_process(&request)?
        };

        match outcome {
            ProcessRemediationOutcome::TerminatedGracefully
            | ProcessRemediationOutcome::TerminatedForced => {
                terminated_count += 1;
                estimated_freed_bytes =
                    estimated_freed_bytes.saturating_add(candidate.identity.current_rss_bytes);
                notify_process_terminated(
                    RemediationReason::Orphan,
                    &candidate.identity,
                    &outcome,
                    NotificationContext {
                        session_state_path: Some(config.notifications.session_state_path.as_path()),
                        ..NotificationContext::default()
                    },
                );
            }
            ProcessRemediationOutcome::AlreadyExited => already_exited_count += 1,
            ProcessRemediationOutcome::Rejected(_) => rejected_count += 1,
        }

        outputs.push(build_output_row(candidate));
    }

    Ok(OrphansOutput {
        mode: mode.to_string(),
        tty_supported,
        matched_count: outputs.len(),
        terminated_count,
        already_exited_count,
        rejected_count,
        estimated_freed_bytes,
        threshold_bytes,
        cycle_index: None,
        processes: outputs,
    })
}

fn render_output(
    mode: &str,
    threshold_bytes: Option<u64>,
    tty_supported: bool,
    candidates: Vec<OrphanCandidate>,
) -> OrphansOutput {
    let total_memory_bytes = candidates.iter().fold(0_u64, |sum, candidate| {
        sum.saturating_add(candidate.identity.current_rss_bytes)
    });
    let processes = candidates
        .into_iter()
        .map(build_output_row)
        .collect::<Vec<_>>();
    OrphansOutput {
        mode: mode.to_string(),
        tty_supported,
        matched_count: processes.len(),
        terminated_count: 0,
        already_exited_count: 0,
        rejected_count: 0,
        estimated_freed_bytes: total_memory_bytes,
        threshold_bytes,
        cycle_index: None,
        processes,
    }
}

fn build_output_row(candidate: OrphanCandidate) -> OrphanProcessOutput {
    OrphanProcessOutput {
        pid: candidate.identity.pid,
        parent_pid: candidate.identity.parent_pid,
        pgid: candidate.identity.pgid,
        memory_bytes: candidate.identity.current_rss_bytes,
        cpu_percent_milli: (candidate.cpu_percent * 1000.0).round() as u32,
        tty: candidate.tty,
        command: candidate.command,
    }
}

fn collect_orphan_candidates(config: &GuardianConfig) -> Result<OrphanScan, OrphanError> {
    let inventory = crate::monitor::process::ProcessInventory::collect_live();
    let tty_map = collect_tty_map()?;
    Ok(scan_with_inventory(config, &inventory, &tty_map))
}

#[derive(Debug, Clone)]
struct OrphanScan {
    tty_supported: bool,
    candidates: Vec<OrphanCandidate>,
}

fn scan_with_inventory(
    config: &GuardianConfig,
    inventory: &crate::monitor::process::ProcessInventory,
    tty_map: &TtyMap,
) -> OrphanScan {
    let ownership_policy = build_orphan_ownership_policy(config);
    let mut candidates = Vec::new();

    for sample in inventory.samples() {
        let Some(tty) = tty_map.by_pid.get(&sample.pid) else {
            continue;
        };
        if !is_orphan_tty(tty) {
            continue;
        }

        if !matches_orphan_command(&sample.command, &ownership_policy.required_command_markers) {
            continue;
        }

        let identity = ProcessIdentity {
            pid: sample.pid,
            parent_pid: sample.parent_pid,
            pgid: sample.pgid,
            start_time_secs: sample.start_time_secs,
            uid: sample.uid,
            current_rss_bytes: sample.memory_bytes,
            command: orphan_identity_command(&sample.command),
            listening_ports: sample.listening_ports.clone(),
        };

        if !matches!(
            validate_process_identity(&identity, &ownership_policy),
            SafetyDecision::Allowed
        ) {
            continue;
        }

        candidates.push(OrphanCandidate {
            identity,
            cpu_percent: sample.cpu_percent,
            tty: Some(tty.clone()),
            command: sample.command.clone(),
        });
    }

    candidates.sort_by(|left, right| {
        right
            .identity
            .current_rss_bytes
            .cmp(&left.identity.current_rss_bytes)
            .then_with(|| left.identity.pid.cmp(&right.identity.pid))
    });

    OrphanScan {
        tty_supported: tty_map.supported,
        candidates,
    }
}

fn build_orphan_ownership_policy(config: &GuardianConfig) -> OwnershipPolicy {
    OwnershipPolicy {
        expected_uid: current_effective_uid(),
        required_command_markers: config.safety.required_command_markers.clone(),
        same_uid_only: config.safety.same_uid_only,
    }
}

#[derive(Debug, Clone, Default)]
struct TtyMap {
    supported: bool,
    by_pid: HashMap<u32, String>,
}

fn collect_tty_map() -> Result<TtyMap, std::io::Error> {
    #[cfg(unix)]
    {
        let output = Command::new("ps").args(["-axo", "pid=,tty="]).output()?;

        if !output.status.success() {
            return Ok(TtyMap::default());
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut by_pid = HashMap::new();
        for line in stdout.lines() {
            let mut parts = line.split_whitespace();
            let Some(pid_raw) = parts.next() else {
                continue;
            };
            let Some(tty) = parts.next() else { continue };
            let Ok(pid) = pid_raw.parse::<u32>() else {
                continue;
            };
            by_pid.insert(pid, tty.to_string());
        }

        Ok(TtyMap {
            supported: true,
            by_pid,
        })
    }

    #[cfg(not(unix))]
    {
        Ok(TtyMap::default())
    }
}

fn is_orphan_tty(tty: &str) -> bool {
    matches!(tty.trim(), "?" | "??")
}

fn matches_orphan_command(command: &str, required_command_markers: &[String]) -> bool {
    if required_command_markers.is_empty() {
        return true;
    }

    command.split_whitespace().map(token_basename).any(|token| {
        required_command_markers
            .iter()
            .any(|marker| token.eq_ignore_ascii_case(marker))
    })
}

fn orphan_identity_command(command: &str) -> String {
    command
        .split_whitespace()
        .map(token_basename)
        .find(|token| matches!(*token, "opencode" | "openagent"))
        .unwrap_or_else(|| command.split_whitespace().next().unwrap_or(command))
        .to_string()
}

fn token_basename(token: &str) -> &str {
    token.rsplit(['/', '\\']).next().unwrap_or(token)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{
        OrphanMode, build_orphan_ownership_policy, is_orphan_tty, matches_orphan_command,
        orphan_identity_command, render_output, scan_with_inventory,
    };
    use crate::config::{GuardianConfig, Mode};
    use crate::monitor::process::{ProcessInventory, ProcessSample};

    fn inventory() -> ProcessInventory {
        ProcessInventory::from_samples([
            ProcessSample {
                pid: 10,
                parent_pid: Some(1),
                pgid: Some(10),
                start_time_secs: 1,
                uid: Some(crate::platform::current_effective_uid()),
                memory_bytes: 512 * 1024 * 1024,
                cpu_percent: 4.2,
                command: "opencode ses_alpha worker".to_string(),
                listening_ports: vec![],
            },
            ProcessSample {
                pid: 11,
                parent_pid: Some(1),
                pgid: Some(11),
                start_time_secs: 2,
                uid: Some(crate::platform::current_effective_uid()),
                memory_bytes: 256 * 1024 * 1024,
                cpu_percent: 0.5,
                command: "bash helper".to_string(),
                listening_ports: vec![],
            },
        ])
    }

    #[test]
    fn is_orphan_tty_matches_shell_style_values() {
        assert!(is_orphan_tty("?"));
        assert!(is_orphan_tty("??"));
        assert!(!is_orphan_tty("pts/1"));
    }

    #[test]
    fn scan_with_inventory_filters_to_marker_and_tty_matches() {
        let config = GuardianConfig::default();
        let tty_map = super::TtyMap {
            supported: true,
            by_pid: [(10_u32, "?".to_string()), (11_u32, "?".to_string())]
                .into_iter()
                .collect(),
        };

        let scan = scan_with_inventory(&config, &inventory(), &tty_map);

        assert!(scan.tty_supported);
        assert_eq!(scan.candidates.len(), 1);
        assert_eq!(scan.candidates[0].identity.pid, 10);
        assert_eq!(scan.candidates[0].identity.command, "opencode");
        assert_eq!(scan.candidates[0].command, "opencode ses_alpha worker");
    }

    #[test]
    fn matches_orphan_command_requires_exact_marker_token_or_basename() {
        let markers = vec!["opencode".to_string(), "openagent".to_string()];

        assert!(matches_orphan_command(
            "opencode ses_alpha worker",
            &markers
        ));
        assert!(matches_orphan_command(
            "/usr/local/bin/opencode ses_alpha worker",
            &markers,
        ));
        assert!(!matches_orphan_command(
            "bun run /Users/name/.config/opencode/skills/server.ts",
            &markers,
        ));
        assert!(!matches_orphan_command(
            "/Users/name/.local/share/opencode/bin/gopls telemetry",
            &markers,
        ));
    }

    #[test]
    fn orphan_identity_command_collapses_to_marker_token() {
        assert_eq!(
            orphan_identity_command("/usr/local/bin/opencode ses_alpha worker"),
            "opencode"
        );
        assert_eq!(
            orphan_identity_command("opencode ses_alpha worker"),
            "opencode"
        );
    }

    #[test]
    fn render_output_sums_memory_for_matches() {
        let config = GuardianConfig::default();
        let tty_map = super::TtyMap {
            supported: true,
            by_pid: [(10_u32, "?".to_string())].into_iter().collect(),
        };
        let scan = scan_with_inventory(&config, &inventory(), &tty_map);

        let output = render_output("list", None, scan.tty_supported, scan.candidates);

        assert_eq!(output.mode, "list");
        assert_eq!(output.matched_count, 1);
        assert_eq!(output.estimated_freed_bytes, 512 * 1024 * 1024);
        assert_eq!(output.processes[0].pid, 10);
    }

    #[test]
    fn build_orphan_ownership_policy_reuses_guardian_safety_settings() {
        let config = GuardianConfig {
            mode: Mode::Enforce,
            ..GuardianConfig::default()
        };
        let policy = build_orphan_ownership_policy(&config);

        assert_eq!(
            policy.expected_uid,
            crate::platform::current_effective_uid()
        );
        assert_eq!(
            policy.required_command_markers,
            config.safety.required_command_markers
        );
        assert_eq!(policy.same_uid_only, config.safety.same_uid_only);
    }

    #[test]
    fn orphan_mode_guard_carries_threshold() {
        let mode = OrphanMode::Guard {
            threshold_bytes: 1024,
            interval: Duration::from_secs(10),
            max_cycles: Some(1),
            force: true,
        };

        match mode {
            OrphanMode::Guard {
                threshold_bytes,
                interval,
                max_cycles,
                force,
            } => {
                assert_eq!(threshold_bytes, 1024);
                assert_eq!(interval, Duration::from_secs(10));
                assert_eq!(max_cycles, Some(1));
                assert!(force);
            }
            _ => panic!("expected guard mode"),
        }
    }
}
