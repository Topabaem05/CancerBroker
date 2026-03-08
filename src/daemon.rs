use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use nix::unistd::geteuid;
use serde::Serialize;

use crate::autocleanup::{AutoCleanupEngine, AutoCleanupSettings};
use crate::cleanup::CleanupPolicy;
use crate::completion::{
    CompletionEvent, CompletionSource, CompletionStateStore, load_completion_state,
    persist_completion_state,
};
use crate::config::GuardianConfig;
use crate::dispatch::CleanupDispatcher;
use crate::ipc::{CompletionEventListener, IpcError, receive_completion_events_once};
use crate::monitor::process::ProcessInventory;
use crate::monitor::storage::scan_allowlisted_roots;
use crate::resolution::{CandidateResolver, SessionArtifactIndex, SessionProcessIndex};
use crate::safety::OwnershipPolicy;

#[derive(Debug, Clone, Serialize)]
pub struct DaemonOutput {
    pub socket_path: PathBuf,
    pub received_events: usize,
    pub processed_events: usize,
    pub reconciled_events: usize,
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
    }
}

fn execution_error(error: impl ToString) -> IpcError {
    IpcError::Execution(error.to_string())
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
        ownership_policy: OwnershipPolicy {
            expected_uid: geteuid().as_raw(),
            required_command_markers: config.safety.required_command_markers.clone(),
            same_uid_only: config.safety.same_uid_only,
        },
        term_timeout: Duration::from_secs(config.completion.cleanup_retry_interval_secs.max(1)),
    }
}

pub async fn run_daemon_once(
    config: &GuardianConfig,
    max_events: usize,
) -> Result<DaemonOutput, IpcError> {
    let mut engine = build_cleanup_engine(config)?;
    let events =
        receive_completion_events_once(&config.completion.daemon_socket_path, max_events).await?;
    let processed_events = process_event_batch(config, &mut engine, &events)?;
    let reconciled_events = run_reconciliation_cycle(config, &mut engine)?;
    persist_engine_state(config, &engine)?;

    Ok(DaemonOutput {
        socket_path: config.completion.daemon_socket_path.clone(),
        received_events: events.len(),
        processed_events,
        reconciled_events,
    })
}

pub async fn run_daemon_loop(
    config: &GuardianConfig,
    options: DaemonRunOptions,
) -> Result<DaemonOutput, IpcError> {
    let mut engine = build_cleanup_engine(config)?;
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
    use std::time::Duration;

    use tempfile::tempdir;

    use super::{
        DaemonRunOptions, build_cleanup_engine, build_cleanup_settings, build_daemon_output,
        build_resolver, execution_error, process_event_batch, run_reconciliation_cycle,
    };
    use crate::completion::{
        CompletionEvent, CompletionRecordState, CompletionSource, CompletionStateEntry,
        CompletionStateSnapshot,
    };
    use crate::config::GuardianConfig;

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

        let mut config = GuardianConfig::default();
        config.storage.allowlist = vec![allowlist.clone()];
        config.completion.state_path = state_path;
        config.completion.cleanup_retry_interval_secs = 9;
        config.sampling.active_session_grace_minutes = 3;

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

        let mut config = GuardianConfig::default();
        config.storage.allowlist = vec![dir.path().to_path_buf()];

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
        let mut config = GuardianConfig::default();
        config.storage.allowlist = vec![dir.path().to_path_buf()];
        config.completion.enabled_sources = vec![CompletionSource::Status];
        config.completion.state_path = dir.path().join("state.json");

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
        let mut config = GuardianConfig::default();
        config.storage.allowlist = vec![dir.path().to_path_buf()];
        config.completion.enabled_sources = vec![CompletionSource::Status];
        config.completion.state_path = dir.path().join("state.json");

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
        let mut config = GuardianConfig::default();
        config.safety.required_command_markers =
            vec!["opencode".to_string(), "subagent".to_string()];
        config.safety.same_uid_only = false;
        config.completion.cleanup_retry_interval_secs = 5;

        let settings = build_cleanup_settings(&config);

        assert_eq!(
            settings.ownership_policy.required_command_markers,
            config.safety.required_command_markers
        );
        assert!(!settings.ownership_policy.same_uid_only);
        assert_eq!(settings.term_timeout, Duration::from_secs(5));
    }
}
