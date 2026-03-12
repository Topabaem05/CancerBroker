use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use serde::Serialize;

use crate::autocleanup::{AutoCleanupEngine, AutoCleanupSettings};
use crate::cleanup::CleanupPolicy;
use crate::completion::{
    CompletionEvent, CompletionSource, CompletionStateStore, load_completion_state,
    persist_completion_state,
};
use crate::config::{GuardianConfig, Mode};
use crate::dispatch::CleanupDispatcher;
use crate::ipc::{CompletionEventListener, IpcError, receive_completion_events_once};
use crate::leak::LeakDetector;
use crate::monitor::process::ProcessInventory;
use crate::monitor::storage::scan_allowlisted_roots;
use crate::platform::current_effective_uid;
use crate::remediation::{
    ProcessGroupRemediationRequest, ProcessRemediationOutcome, ProcessRemediationRequest,
    remediate_process, remediate_process_group,
};
use crate::resolution::{
    CandidateResolver, SessionArtifactIndex, SessionPortIndex, SessionProcessIndex,
};
use crate::safety::OwnershipPolicy;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct LeakCleanupOutput {
    pub leak_candidates: usize,
    pub leak_process_remediations: usize,
    pub leak_group_remediations: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct DaemonOutput {
    pub socket_path: PathBuf,
    pub received_events: usize,
    pub processed_events: usize,
    pub reconciled_events: usize,
    pub leak_candidates: usize,
    pub leak_process_remediations: usize,
    pub leak_group_remediations: usize,
}

#[derive(Debug, Clone, Copy)]
pub struct DaemonRunOptions {
    pub max_events_per_batch: usize,
    pub max_cycles: Option<usize>,
    pub idle_timeout: Duration,
}

impl Default for DaemonRunOptions {
    fn default() -> Self {
        Self {
            max_events_per_batch: 1,
            max_cycles: None,
            idle_timeout: Duration::from_secs(60),
        }
    }
}

fn build_daemon_output(socket_path: PathBuf) -> DaemonOutput {
    DaemonOutput {
        socket_path,
        received_events: 0,
        processed_events: 0,
        reconciled_events: 0,
        leak_candidates: 0,
        leak_process_remediations: 0,
        leak_group_remediations: 0,
    }
}

fn execution_error(error: impl ToString) -> IpcError {
    IpcError::Execution(error.to_string())
}

fn build_ownership_policy(config: &GuardianConfig) -> OwnershipPolicy {
    OwnershipPolicy {
        expected_uid: current_effective_uid(),
        required_command_markers: config.safety.required_command_markers.clone(),
        same_uid_only: config.safety.same_uid_only,
    }
}

fn build_cleanup_settings(config: &GuardianConfig) -> AutoCleanupSettings {
    AutoCleanupSettings {
        cleanup_policy: CleanupPolicy {
            allowlist: config.storage.allowlist.clone(),
            active_session_grace: Duration::from_secs(
                config
                    .sampling
                    .active_session_grace_minutes
                    .saturating_mul(60),
            ),
        },
        ownership_policy: build_ownership_policy(config),
        term_timeout: Duration::from_secs(config.completion.cleanup_retry_interval_secs.max(1)),
    }
}

fn remediation_succeeded(outcome: &ProcessRemediationOutcome) -> bool {
    matches!(
        outcome,
        ProcessRemediationOutcome::TerminatedGracefully
            | ProcessRemediationOutcome::TerminatedForced
    )
}

fn run_leak_enforcement_with_inventory(
    config: &GuardianConfig,
    detector: &mut LeakDetector,
    inventory: &ProcessInventory,
) -> Result<LeakCleanupOutput, IpcError> {
    let ownership_policy = build_ownership_policy(config);
    let candidates =
        detector.observe_inventory(inventory, &config.leak_detection, &ownership_policy);

    if config.mode != Mode::Enforce {
        return Ok(LeakCleanupOutput {
            leak_candidates: candidates.len(),
            ..LeakCleanupOutput::default()
        });
    }

    let term_timeout = Duration::from_secs(config.completion.cleanup_retry_interval_secs.max(1));
    let mut leak_process_remediations = 0;
    let mut leak_group_remediations = 0;
    let mut seen_pgids = std::collections::BTreeSet::new();

    for candidate in &candidates {
        let outcome = remediate_process(&ProcessRemediationRequest {
            identity: candidate.identity.clone(),
            ownership_policy: ownership_policy.clone(),
            term_timeout,
        })
        .map_err(execution_error)?;

        if remediation_succeeded(&outcome) {
            leak_process_remediations += 1;
        }

        if let Some(pgid) = candidate.identity.pgid
            && seen_pgids.insert(pgid)
        {
            let outcome = remediate_process_group(&ProcessGroupRemediationRequest {
                pgid,
                leader_identity: candidate.identity.clone(),
                ownership_policy: ownership_policy.clone(),
                term_timeout,
            })
            .map_err(execution_error)?;

            if remediation_succeeded(&outcome) {
                leak_group_remediations += 1;
            }
        }
    }

    Ok(LeakCleanupOutput {
        leak_candidates: candidates.len(),
        leak_process_remediations,
        leak_group_remediations,
    })
}

fn run_leak_enforcement_cycle(
    config: &GuardianConfig,
    detector: &mut LeakDetector,
) -> Result<LeakCleanupOutput, IpcError> {
    let inventory = ProcessInventory::collect_live();
    run_leak_enforcement_with_inventory(config, detector, &inventory)
}

pub async fn run_daemon_once(
    config: &GuardianConfig,
    max_events: usize,
) -> Result<DaemonOutput, IpcError> {
    let mut engine = build_cleanup_engine(config)?;
    let mut detector = LeakDetector::default();
    let events =
        receive_completion_events_once(&config.completion.daemon_socket_path, max_events).await?;
    let processed_events = process_event_batch(config, &mut engine, &events)?;
    let reconciled_events = run_reconciliation_cycle(config, &mut engine)?;
    let leak_output = run_leak_enforcement_cycle(config, &mut detector)?;
    persist_engine_state(config, &engine)?;

    Ok(DaemonOutput {
        socket_path: config.completion.daemon_socket_path.clone(),
        received_events: events.len(),
        processed_events,
        reconciled_events,
        leak_candidates: leak_output.leak_candidates,
        leak_process_remediations: leak_output.leak_process_remediations,
        leak_group_remediations: leak_output.leak_group_remediations,
    })
}

pub async fn run_daemon_loop(
    config: &GuardianConfig,
    options: DaemonRunOptions,
) -> Result<DaemonOutput, IpcError> {
    let mut engine = build_cleanup_engine(config)?;
    let mut detector = LeakDetector::default();
    let listener = CompletionEventListener::bind(&config.completion.daemon_socket_path)?;
    let mut output = build_daemon_output(config.completion.daemon_socket_path.clone());
    let mut cycles = 0_usize;

    loop {
        let events = listener
            .receive_batch(options.max_events_per_batch, Some(options.idle_timeout))
            .await?;
        output.received_events += events.len();
        output.processed_events += process_event_batch(config, &mut engine, &events)?;
        output.reconciled_events += run_reconciliation_cycle(config, &mut engine)?;
        let leak_output = run_leak_enforcement_cycle(config, &mut detector)?;
        output.leak_candidates += leak_output.leak_candidates;
        output.leak_process_remediations += leak_output.leak_process_remediations;
        output.leak_group_remediations += leak_output.leak_group_remediations;
        persist_engine_state(config, &engine)?;

        cycles += 1;
        if options.max_cycles.is_some_and(|limit| cycles >= limit) {
            break;
        }
    }

    Ok(output)
}

fn build_cleanup_engine(config: &GuardianConfig) -> Result<AutoCleanupEngine, IpcError> {
    let resolver = build_resolver(config)?;
    let state = load_completion_state(
        &config.completion.state_path,
        config.completion.dedupe_ttl_secs,
    )
    .map_err(execution_error)?;

    Ok(AutoCleanupEngine::new(
        CleanupDispatcher::new(state, resolver),
        build_cleanup_settings(config),
    ))
}

fn build_resolver(config: &GuardianConfig) -> Result<CandidateResolver, IpcError> {
    let snapshot = scan_allowlisted_roots(&config.storage.allowlist).map_err(execution_error)?;
    let inventory = ProcessInventory::collect_live();

    Ok(CandidateResolver::new(
        SessionProcessIndex::from_inventory(&inventory),
        SessionArtifactIndex::from_snapshot(&snapshot),
        SessionPortIndex::from_inventory(&inventory),
    ))
}

fn process_event_batch(
    config: &GuardianConfig,
    engine: &mut AutoCleanupEngine,
    events: &[CompletionEvent],
) -> Result<usize, IpcError> {
    engine.set_resolver(build_resolver(config)?);

    let mut processed_events = 0;
    for event in events {
        if !config.completion.enabled_sources.contains(&event.source) {
            continue;
        }

        engine
            .handle_completion_event(event, SystemTime::now())
            .map_err(execution_error)?;
        processed_events += 1;
    }

    Ok(processed_events)
}

fn run_reconciliation_cycle(
    config: &GuardianConfig,
    engine: &mut AutoCleanupEngine,
) -> Result<usize, IpcError> {
    if !config
        .completion
        .enabled_sources
        .contains(&CompletionSource::Inferred)
    {
        return Ok(0);
    }

    engine.set_resolver(build_resolver(config)?);
    engine
        .run_reconciliation_pass(SystemTime::now())
        .map(|outcomes| outcomes.len())
        .map_err(execution_error)
}

fn persist_engine_state(
    config: &GuardianConfig,
    engine: &AutoCleanupEngine,
) -> Result<(), IpcError> {
    let state = CompletionStateStore::from_snapshot(
        config.completion.dedupe_ttl_secs,
        engine.state_snapshot(),
    );
    persist_completion_state(&config.completion.state_path, &state).map_err(execution_error)
}

#[cfg(test)]
mod tests {
    use std::fs;
    #[cfg(any(unix, windows))]
    use std::process::Command;
    use std::time::Duration;

    use tempfile::tempdir;

    use super::{
        DaemonRunOptions, build_cleanup_engine, build_cleanup_settings, build_daemon_output,
        build_ownership_policy, build_resolver, execution_error, process_event_batch,
        remediation_succeeded, run_leak_enforcement_with_inventory, run_reconciliation_cycle,
    };
    use crate::completion::{
        CompletionEvent, CompletionRecordState, CompletionSource, CompletionStateEntry,
        CompletionStateSnapshot,
    };
    use crate::config::{
        CompletionCleanupPolicy, GuardianConfig, LeakDetectionPolicy, Mode, SafetyPolicy,
        SamplingPolicy, StoragePolicy,
    };
    use crate::leak::LeakDetector;
    use crate::monitor::process::{ProcessInventory, ProcessSample};
    use crate::platform::current_effective_uid;
    use crate::remediation::ProcessRemediationOutcome;

    fn status_event(session_id: &str, source: CompletionSource) -> CompletionEvent {
        CompletionEvent {
            event_id: format!("evt-{session_id}"),
            session_id: Some(session_id.to_string()),
            parent_session_id: None,
            task_id: None,
            tool_name: None,
            completed_at: "2026-03-08T20:00:00Z".to_string(),
            source,
        }
    }

    #[test]
    fn daemon_run_options_default_to_single_event_batches() {
        let options = DaemonRunOptions::default();

        assert_eq!(options.max_events_per_batch, 1);
        assert_eq!(options.max_cycles, None);
        assert_eq!(options.idle_timeout, Duration::from_secs(60));
    }

    #[test]
    fn build_daemon_output_starts_counters_at_zero() {
        let output = build_daemon_output("/tmp/test.sock".into());

        assert_eq!(output.socket_path.to_string_lossy(), "/tmp/test.sock");
        assert_eq!(output.received_events, 0);
        assert_eq!(output.processed_events, 0);
        assert_eq!(output.reconciled_events, 0);
        assert_eq!(output.leak_candidates, 0);
        assert_eq!(output.leak_process_remediations, 0);
        assert_eq!(output.leak_group_remediations, 0);
    }

    #[test]
    fn build_cleanup_engine_uses_configured_state_and_settings() {
        let dir = tempdir().expect("tempdir");
        let state_path = dir.path().join("completion-state.json");
        let allowlist = dir.path().join("storage");
        fs::create_dir_all(&allowlist).expect("allowlist should exist");
        fs::write(
            &state_path,
            serde_json::to_string(&CompletionStateSnapshot {
                entries: vec![CompletionStateEntry {
                    dedupe_key: "evt:ses:-:status".to_string(),
                    updated_at_unix_secs: 12,
                    state: CompletionRecordState::Processed,
                }],
            })
            .expect("snapshot should serialize"),
        )
        .expect("state file should be written");

        let config = GuardianConfig {
            storage: StoragePolicy {
                allowlist: vec![allowlist.clone()],
            },
            sampling: SamplingPolicy {
                active_session_grace_minutes: 3,
                ..SamplingPolicy::default()
            },
            completion: CompletionCleanupPolicy {
                state_path,
                cleanup_retry_interval_secs: 9,
                ..CompletionCleanupPolicy::default()
            },
            ..GuardianConfig::default()
        };

        let engine = build_cleanup_engine(&config).expect("engine should build");
        let settings = build_cleanup_settings(&config);

        assert_eq!(engine.state_snapshot().entries.len(), 1);
        assert_eq!(settings.cleanup_policy.allowlist, vec![allowlist]);
        assert_eq!(
            settings.cleanup_policy.active_session_grace,
            Duration::from_secs(180)
        );
        assert_eq!(settings.term_timeout, Duration::from_secs(9));
    }

    #[test]
    fn build_resolver_finds_allowlisted_artifacts_for_session_ids() {
        let dir = tempdir().expect("tempdir");
        let artifact = dir.path().join("ses_alpha_artifact.json");
        fs::write(&artifact, "{}").expect("artifact should be written");

        let config = GuardianConfig {
            storage: StoragePolicy {
                allowlist: vec![dir.path().to_path_buf()],
            },
            ..GuardianConfig::default()
        };

        let resolver = build_resolver(&config).expect("resolver should build");
        let resolved = resolver.resolve(&status_event(
            "ses_alpha_artifact",
            CompletionSource::Status,
        ));

        assert_eq!(resolved.artifacts, vec![artifact]);
        assert!(resolved.immediate_cleanup_eligible);
    }

    #[test]
    fn process_event_batch_skips_disabled_sources() {
        let dir = tempdir().expect("tempdir");
        let config = GuardianConfig {
            storage: StoragePolicy {
                allowlist: vec![dir.path().to_path_buf()],
            },
            completion: CompletionCleanupPolicy {
                enabled_sources: vec![CompletionSource::Status],
                state_path: dir.path().join("state.json"),
                ..CompletionCleanupPolicy::default()
            },
            ..GuardianConfig::default()
        };

        let mut engine = build_cleanup_engine(&config).expect("engine should build");
        let processed = process_event_batch(
            &config,
            &mut engine,
            &[status_event("ses_alpha", CompletionSource::Deleted)],
        )
        .expect("disabled source should not fail");

        assert_eq!(processed, 0);
    }

    #[test]
    fn run_reconciliation_cycle_skips_when_inferred_source_disabled() {
        let dir = tempdir().expect("tempdir");
        let config = GuardianConfig {
            storage: StoragePolicy {
                allowlist: vec![dir.path().to_path_buf()],
            },
            completion: CompletionCleanupPolicy {
                enabled_sources: vec![CompletionSource::Status],
                state_path: dir.path().join("state.json"),
                ..CompletionCleanupPolicy::default()
            },
            ..GuardianConfig::default()
        };

        let mut engine = build_cleanup_engine(&config).expect("engine should build");
        let reconciled = run_reconciliation_cycle(&config, &mut engine)
            .expect("reconciliation should skip cleanly");

        assert_eq!(reconciled, 0);
    }

    #[test]
    fn execution_error_wraps_displayable_sources() {
        let error = execution_error("boom");

        assert_eq!(error.to_string(), "execution_failed: boom");
    }

    #[test]
    fn build_cleanup_settings_uses_current_uid_and_markers() {
        let config = GuardianConfig {
            safety: SafetyPolicy {
                required_command_markers: vec!["opencode".to_string(), "subagent".to_string()],
                same_uid_only: false,
            },
            completion: CompletionCleanupPolicy {
                cleanup_retry_interval_secs: 5,
                ..CompletionCleanupPolicy::default()
            },
            ..GuardianConfig::default()
        };

        let settings = build_cleanup_settings(&config);
        let ownership_policy = build_ownership_policy(&config);

        assert_eq!(
            settings.ownership_policy.required_command_markers,
            config.safety.required_command_markers
        );
        assert!(!settings.ownership_policy.same_uid_only);
        assert_eq!(settings.term_timeout, Duration::from_secs(5));
        assert_eq!(ownership_policy, settings.ownership_policy);
    }

    fn leak_test_inventory(pid: u32, start_time_secs: u64, memory_bytes: u64) -> ProcessInventory {
        ProcessInventory::from_samples([ProcessSample {
            pid,
            parent_pid: Some(1),
            pgid: None,
            start_time_secs,
            uid: Some(current_effective_uid()),
            memory_bytes,
            cpu_percent: 0.5,
            command: "sleep leak".to_string(),
            listening_ports: vec![],
        }])
    }

    #[test]
    fn run_leak_enforcement_with_inventory_reports_candidates_in_observe_mode() {
        let config = GuardianConfig {
            mode: Mode::Observe,
            safety: SafetyPolicy {
                required_command_markers: vec!["sleep".to_string()],
                same_uid_only: true,
            },
            leak_detection: LeakDetectionPolicy {
                enabled: true,
                required_consecutive_growth_samples: 2,
                minimum_rss_bytes: 100,
                minimum_growth_bytes_per_sample: 20,
            },
            ..GuardianConfig::default()
        };
        let mut detector = LeakDetector::default();

        assert_eq!(
            run_leak_enforcement_with_inventory(
                &config,
                &mut detector,
                &leak_test_inventory(77, 42, 100)
            )
            .expect("first sample"),
            super::LeakCleanupOutput::default()
        );
        assert_eq!(
            run_leak_enforcement_with_inventory(
                &config,
                &mut detector,
                &leak_test_inventory(77, 42, 130)
            )
            .expect("second sample"),
            super::LeakCleanupOutput::default()
        );

        let output = run_leak_enforcement_with_inventory(
            &config,
            &mut detector,
            &leak_test_inventory(77, 42, 160),
        )
        .expect("third sample");

        assert_eq!(output.leak_candidates, 1);
        assert_eq!(output.leak_process_remediations, 0);
        assert_eq!(output.leak_group_remediations, 0);
    }

    #[cfg(unix)]
    #[test]
    fn run_leak_enforcement_with_inventory_terminates_leaking_process_in_enforce_mode() {
        let mut child = Command::new("sleep")
            .arg("30")
            .spawn()
            .expect("sleep process should spawn");

        let config = GuardianConfig {
            mode: Mode::Enforce,
            safety: SafetyPolicy {
                required_command_markers: vec!["sleep".to_string()],
                same_uid_only: true,
            },
            leak_detection: LeakDetectionPolicy {
                enabled: true,
                required_consecutive_growth_samples: 2,
                minimum_rss_bytes: 100,
                minimum_growth_bytes_per_sample: 20,
            },
            completion: CompletionCleanupPolicy {
                cleanup_retry_interval_secs: 1,
                ..CompletionCleanupPolicy::default()
            },
            ..GuardianConfig::default()
        };
        let mut detector = LeakDetector::default();

        for memory_bytes in [100_u64, 130, 160] {
            let _ = run_leak_enforcement_with_inventory(
                &config,
                &mut detector,
                &leak_test_inventory(child.id(), 42, memory_bytes),
            )
            .expect("leak enforcement should run");
        }

        let status = child.wait().expect("child should exit after remediation");
        assert!(!status.success());
    }

    #[cfg(windows)]
    #[test]
    fn run_leak_enforcement_with_inventory_terminates_leaking_process_in_enforce_mode() {
        let mut child = Command::new("ping")
            .args(["-n", "30", "127.0.0.1"])
            .spawn()
            .expect("ping process should spawn");

        let config = GuardianConfig {
            mode: Mode::Enforce,
            safety: SafetyPolicy {
                required_command_markers: vec!["sleep".to_string()],
                same_uid_only: false,
            },
            leak_detection: LeakDetectionPolicy {
                enabled: true,
                required_consecutive_growth_samples: 2,
                minimum_rss_bytes: 100,
                minimum_growth_bytes_per_sample: 20,
            },
            completion: CompletionCleanupPolicy {
                cleanup_retry_interval_secs: 1,
                ..CompletionCleanupPolicy::default()
            },
            ..GuardianConfig::default()
        };
        let mut detector = LeakDetector::default();

        for memory_bytes in [100_u64, 130, 160] {
            let _ = run_leak_enforcement_with_inventory(
                &config,
                &mut detector,
                &leak_test_inventory(child.id(), 42, memory_bytes),
            )
            .expect("leak enforcement should run");
        }

        let status = child.wait().expect("child should exit after remediation");
        assert!(!status.success());
    }

    #[test]
    fn remediation_succeeded_only_counts_terminated_outcomes() {
        assert!(remediation_succeeded(
            &ProcessRemediationOutcome::TerminatedGracefully
        ));
        assert!(remediation_succeeded(
            &ProcessRemediationOutcome::TerminatedForced
        ));
        assert!(!remediation_succeeded(
            &ProcessRemediationOutcome::AlreadyExited
        ));
        assert!(!remediation_succeeded(
            &ProcessRemediationOutcome::Rejected("uid_mismatch")
        ));
    }
}
