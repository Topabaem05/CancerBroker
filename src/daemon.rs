use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use nix::unistd::geteuid;
use serde::Serialize;

use crate::autocleanup::{AutoCleanupEngine, AutoCleanupSettings};
use crate::cleanup::CleanupPolicy;
use crate::completion::CompletionStateStore;
use crate::config::GuardianConfig;
use crate::dispatch::CleanupDispatcher;
use crate::ipc::{IpcError, receive_completion_events_once};
use crate::monitor::storage::scan_allowlisted_roots;
use crate::resolution::{CandidateResolver, SessionArtifactIndex, SessionProcessIndex};
use crate::safety::OwnershipPolicy;

#[derive(Debug, Clone, Serialize)]
pub struct DaemonOutput {
    pub socket_path: PathBuf,
    pub received_events: usize,
    pub processed_events: usize,
}

pub async fn run_daemon_once(
    config: &GuardianConfig,
    max_events: usize,
) -> Result<DaemonOutput, IpcError> {
    let snapshot = scan_allowlisted_roots(&config.storage.allowlist)
        .map_err(|error| IpcError::Execution(error.to_string()))?;
    let resolver = CandidateResolver::new(
        SessionProcessIndex::default(),
        SessionArtifactIndex::from_snapshot(&snapshot),
    );
    let mut engine = AutoCleanupEngine::new(
        CleanupDispatcher::new(
            CompletionStateStore::new(config.completion.dedupe_ttl_secs),
            resolver,
        ),
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
    );

    let events =
        receive_completion_events_once(&config.completion.daemon_socket_path, max_events).await?;
    let mut processed_events = 0;

    for event in &events {
        if !config.completion.enabled_sources.contains(&event.source) {
            continue;
        }

        engine
            .handle_completion_event(event, SystemTime::now())
            .map_err(|error| IpcError::Execution(error.to_string()))?;
        processed_events += 1;
    }

    Ok(DaemonOutput {
        socket_path: config.completion.daemon_socket_path.clone(),
        received_events: events.len(),
        processed_events,
    })
}
