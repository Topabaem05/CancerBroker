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
    let mut output = DaemonOutput {
        socket_path: config.completion.daemon_socket_path.clone(),
        received_events: 0,
        processed_events: 0,
        reconciled_events: 0,
    };
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
    .map_err(|error| IpcError::Execution(error.to_string()))?;

    Ok(AutoCleanupEngine::new(
        CleanupDispatcher::new(state, resolver),
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
        },
    ))
}

fn build_resolver(config: &GuardianConfig) -> Result<CandidateResolver, IpcError> {
    let snapshot = scan_allowlisted_roots(&config.storage.allowlist)
        .map_err(|error| IpcError::Execution(error.to_string()))?;

    Ok(CandidateResolver::new(
        SessionProcessIndex::default(),
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
            .map_err(|error| IpcError::Execution(error.to_string()))?;
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
        .map_err(|error| IpcError::Execution(error.to_string()))
}

fn persist_engine_state(
    config: &GuardianConfig,
    engine: &AutoCleanupEngine,
) -> Result<(), IpcError> {
    let state = CompletionStateStore::from_snapshot(
        config.completion.dedupe_ttl_secs,
        engine.state_snapshot(),
    );
    persist_completion_state(&config.completion.state_path, &state)
        .map_err(|error| IpcError::Execution(error.to_string()))
}
