use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, SystemTime};

use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use serde::Serialize;

use crate::autocleanup::{AutoCleanupEngine, AutoCleanupSettings};
use crate::cleanup::CleanupPolicy;
use crate::completion::{
    CompletionEvent, CompletionSource, CompletionStateStore, load_completion_state,
    persist_completion_state,
};
use crate::config::{GuardianConfig, Mode, RustAnalyzerMemoryGuardPolicy};
use crate::dispatch::CleanupDispatcher;
use crate::ipc::{CompletionEventListener, IpcError, receive_completion_events_once};
use crate::leak::LeakDetector;
use crate::memory_guard::RustAnalyzerMemoryGuard;
use crate::monitor::process::ProcessInventory;
use crate::monitor::storage::{
    StorageSnapshot, scan_allowlisted_roots, try_apply_watch_events_incremental,
};
use crate::notifications::{
    RemediationReason, notify_process_group_terminated, notify_process_terminated,
};
use crate::platform::current_effective_uid;
use crate::remediation::{
    ProcessGroupRemediationRequest, ProcessRemediationOutcome, ProcessRemediationRequest,
    remediate_process, remediate_process_group,
};
use crate::resolution::{
    CandidateResolver, SessionArtifactIndex, SessionPortIndex, SessionProcessIndex,
    session_ids_in_text,
};
use crate::safety::OwnershipPolicy;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct LeakCleanupOutput {
    pub leak_candidates: usize,
    pub leak_process_remediations: usize,
    pub leak_group_remediations: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct MemoryGuardOutput {
    pub rust_analyzer_memory_candidates: usize,
    pub rust_analyzer_memory_remediations: usize,
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
    pub rust_analyzer_memory_candidates: usize,
    pub rust_analyzer_memory_remediations: usize,
}

#[derive(Debug, Clone, Copy)]
pub struct DaemonRunOptions {
    pub max_events_per_batch: usize,
    pub max_cycles: Option<usize>,
    pub idle_timeout: Duration,
}

struct StorageSnapshotCache {
    allowlisted_roots: Vec<PathBuf>,
    snapshot: StorageSnapshot,
    _watcher: Option<RecommendedWatcher>,
    watch_events: Option<Receiver<notify::Result<notify::Event>>>,
    snapshot_used: bool,
    fallback_rescan_each_cycle: bool,
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
        rust_analyzer_memory_candidates: 0,
        rust_analyzer_memory_remediations: 0,
    }
}

fn build_storage_watcher(
    allowlisted_roots: &[PathBuf],
) -> notify::Result<(RecommendedWatcher, Receiver<notify::Result<notify::Event>>)> {
    let (tx, rx) = mpsc::channel();
    let mut watcher = notify::recommended_watcher(move |result| {
        let _ = tx.send(result);
    })?;

    for root in allowlisted_roots {
        watcher.watch(root, RecursiveMode::Recursive)?;
    }

    Ok((watcher, rx))
}

impl StorageSnapshotCache {
    fn new(allowlisted_roots: &[PathBuf]) -> Result<Self, IpcError> {
        let snapshot = scan_allowlisted_roots(allowlisted_roots).map_err(execution_error)?;
        let watchable_roots = !allowlisted_roots.is_empty()
            && allowlisted_roots
                .iter()
                .all(|root| root.exists() && root.is_dir());
        let (watcher, watch_events, fallback_rescan_each_cycle) = if watchable_roots {
            match build_storage_watcher(allowlisted_roots) {
                Ok((watcher, watch_events)) => (Some(watcher), Some(watch_events), false),
                Err(_) => (None, None, true),
            }
        } else {
            (None, None, true)
        };

        Ok(Self {
            allowlisted_roots: allowlisted_roots.to_vec(),
            snapshot,
            _watcher: watcher,
            watch_events,
            snapshot_used: false,
            fallback_rescan_each_cycle,
        })
    }

    fn refresh_if_needed(&mut self) -> Result<&StorageSnapshot, IpcError> {
        let requires_rescan = self.apply_pending_watch_events();

        if self.snapshot_used {
            if self.fallback_rescan_each_cycle || requires_rescan {
                self.snapshot =
                    scan_allowlisted_roots(&self.allowlisted_roots).map_err(execution_error)?;
            }
        } else {
            self.snapshot_used = true;
            if requires_rescan {
                self.snapshot =
                    scan_allowlisted_roots(&self.allowlisted_roots).map_err(execution_error)?;
            }
        }

        Ok(&self.snapshot)
    }

    fn apply_pending_watch_events(&mut self) -> bool {
        if self.fallback_rescan_each_cycle {
            return self.snapshot_used;
        }

        let Some(events) = self.take_watch_events() else {
            return false;
        };

        if try_apply_watch_events_incremental(&mut self.snapshot, &events) {
            return false;
        }

        true
    }

    fn take_watch_events(&mut self) -> Option<Vec<Event>> {
        let Some(watch_events) = &self.watch_events else {
            return None;
        };

        let mut events = Vec::new();
        while let Ok(result) = watch_events.try_recv() {
            match result {
                Ok(event) => events.push(event),
                Err(_) => {
                    self.fallback_rescan_each_cycle = true;
                    self.watch_events = None;
                    self._watcher = None;
                    return Some(Vec::new());
                }
            }
        }

        if events.is_empty() {
            None
        } else {
            Some(events)
        }
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

fn build_rust_analyzer_ownership_policy(config: &GuardianConfig) -> OwnershipPolicy {
    OwnershipPolicy {
        expected_uid: current_effective_uid(),
        required_command_markers: vec!["rust-analyzer".to_string()],
        same_uid_only: config.rust_analyzer_memory_guard.same_uid_only,
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
    outcome.was_terminated()
}

fn command_matches_markers(command: &str, markers: &[String]) -> bool {
    if markers.is_empty() {
        return false;
    }

    let command = command.to_lowercase();
    markers
        .iter()
        .any(|marker| command.contains(&marker.to_lowercase()))
}

fn build_opencode_related_sets(
    inventory: &ProcessInventory,
    markers: &[String],
) -> (
    std::collections::BTreeSet<u32>,
    std::collections::BTreeSet<u32>,
) {
    let mut related_pids = std::collections::BTreeSet::new();
    let mut related_pgids = std::collections::BTreeSet::new();

    for sample in inventory.samples() {
        if command_matches_markers(&sample.command, markers) {
            related_pids.insert(sample.pid);
            if let Some(pgid) = sample.pgid {
                related_pgids.insert(pgid);
            }
        }
    }

    (related_pids, related_pgids)
}

fn is_opencode_related_identity(
    identity: &crate::safety::ProcessIdentity,
    markers: &[String],
    related_pids: &std::collections::BTreeSet<u32>,
    related_pgids: &std::collections::BTreeSet<u32>,
) -> bool {
    command_matches_markers(&identity.command, markers)
        || identity
            .parent_pid
            .is_some_and(|pid| related_pids.contains(&pid))
        || identity
            .pgid
            .is_some_and(|pgid| related_pgids.contains(&pgid))
}

fn notification_session_id(identity: &crate::safety::ProcessIdentity) -> Option<String> {
    session_ids_in_text(&identity.command).next()
}

fn run_leak_enforcement_with_inventory(
    config: &GuardianConfig,
    detector: &mut LeakDetector,
    inventory: &ProcessInventory,
    ownership_policy: &OwnershipPolicy,
    term_timeout: Duration,
) -> Result<LeakCleanupOutput, IpcError> {
    let candidates =
        detector.observe_inventory(inventory, &config.leak_detection, ownership_policy);

    if config.mode != Mode::Enforce {
        return Ok(LeakCleanupOutput {
            leak_candidates: candidates.len(),
            ..LeakCleanupOutput::default()
        });
    }

    let mut leak_process_remediations = 0;
    let mut leak_group_remediations = 0;
    let mut seen_pgids = std::collections::BTreeSet::new();
    let mut successful_group_notifications = std::collections::BTreeSet::new();
    let mut successful_processes = Vec::new();
    let mut successful_groups = Vec::new();
    let (related_pids, related_pgids) =
        build_opencode_related_sets(inventory, &ownership_policy.required_command_markers);

    for candidate in &candidates {
        let outcome = remediate_process(&ProcessRemediationRequest {
            identity: candidate.identity.clone(),
            ownership_policy: ownership_policy.clone(),
            term_timeout,
        })
        .map_err(execution_error)?;

        if remediation_succeeded(&outcome) {
            leak_process_remediations += 1;
            if is_opencode_related_identity(
                &candidate.identity,
                &ownership_policy.required_command_markers,
                &related_pids,
                &related_pgids,
            ) {
                successful_processes.push((candidate.identity.clone(), outcome.clone()));
            }
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
                if is_opencode_related_identity(
                    &candidate.identity,
                    &ownership_policy.required_command_markers,
                    &related_pids,
                    &related_pgids,
                ) {
                    successful_group_notifications.insert(pgid);
                    successful_groups.push((pgid, candidate.identity.clone(), outcome.clone()));
                }
            }
        }
    }

    for (pgid, leader_identity, outcome) in &successful_groups {
        let session_id = notification_session_id(leader_identity);
        notify_process_group_terminated(
            RemediationReason::Leak,
            *pgid,
            leader_identity,
            outcome,
            session_id.as_deref(),
        );
    }

    for (identity, outcome) in &successful_processes {
        if identity
            .pgid
            .is_some_and(|pgid| successful_group_notifications.contains(&pgid))
        {
            continue;
        }
        let session_id = notification_session_id(identity);
        notify_process_terminated(
            RemediationReason::Leak,
            identity,
            outcome,
            session_id.as_deref(),
        );
    }

    Ok(LeakCleanupOutput {
        leak_candidates: candidates.len(),
        leak_process_remediations,
        leak_group_remediations,
    })
}

fn run_rust_analyzer_memory_guard_with_inventory(
    config: &GuardianConfig,
    guard: &mut RustAnalyzerMemoryGuard,
    inventory: &ProcessInventory,
    term_timeout: Duration,
    now: SystemTime,
) -> Result<MemoryGuardOutput, IpcError> {
    let policy: &RustAnalyzerMemoryGuardPolicy = &config.rust_analyzer_memory_guard;
    let ownership_policy = build_rust_analyzer_ownership_policy(config);
    let candidates = guard.observe_inventory(inventory, policy, &ownership_policy, now);

    if config.mode != Mode::Enforce {
        return Ok(MemoryGuardOutput {
            rust_analyzer_memory_candidates: candidates.len(),
            ..MemoryGuardOutput::default()
        });
    }

    let mut remediations = 0;
    for candidate in &candidates {
        let outcome = remediate_process(&ProcessRemediationRequest {
            identity: candidate.identity.clone(),
            ownership_policy: ownership_policy.clone(),
            term_timeout,
        })
        .map_err(execution_error)?;

        if remediation_succeeded(&outcome) {
            remediations += 1;
        }
    }

    if remediations > 0 {
        guard.record_remediation(now);
    }

    Ok(MemoryGuardOutput {
        rust_analyzer_memory_candidates: candidates.len(),
        rust_analyzer_memory_remediations: remediations,
    })
}

fn run_rust_analyzer_memory_guard_once_with_inventory(
    config: &GuardianConfig,
    guard: &mut RustAnalyzerMemoryGuard,
    inventory: &ProcessInventory,
    now: SystemTime,
) -> Result<MemoryGuardOutput, IpcError> {
    let term_timeout = Duration::from_secs(config.completion.cleanup_retry_interval_secs.max(1));
    run_rust_analyzer_memory_guard_with_inventory(config, guard, inventory, term_timeout, now)
}

pub fn run_rust_analyzer_memory_guard_once(
    config: &GuardianConfig,
) -> Result<MemoryGuardOutput, IpcError> {
    let inventory = ProcessInventory::collect_live_for_rust_analyzer_guard();
    let mut guard = RustAnalyzerMemoryGuard::default();
    run_rust_analyzer_memory_guard_once_with_inventory(
        config,
        &mut guard,
        &inventory,
        SystemTime::now(),
    )
}

pub async fn run_daemon_once(
    config: &GuardianConfig,
    max_events: usize,
) -> Result<DaemonOutput, IpcError> {
    let ownership_policy = build_ownership_policy(config);
    let term_timeout = Duration::from_secs(config.completion.cleanup_retry_interval_secs.max(1));
    let inventory = ProcessInventory::collect_live();
    let snapshot = scan_allowlisted_roots(&config.storage.allowlist).map_err(execution_error)?;
    let mut engine =
        build_cleanup_engine_with_resolver(config, build_resolver_from(&inventory, &snapshot))?;
    let mut detector = LeakDetector::default();
    let mut rust_analyzer_guard = RustAnalyzerMemoryGuard::default();
    let events =
        receive_completion_events_once(&config.completion.daemon_socket_path, max_events).await?;

    let now = SystemTime::now();
    let processed_events = process_event_batch(config, &mut engine, &events)?;
    let reconciled_events = run_reconciliation_cycle(config, &mut engine, &snapshot, now)?;
    let leak_output = run_leak_enforcement_with_inventory(
        config,
        &mut detector,
        &inventory,
        &ownership_policy,
        term_timeout,
    )?;
    let memory_guard_output = run_rust_analyzer_memory_guard_with_inventory(
        config,
        &mut rust_analyzer_guard,
        &inventory,
        term_timeout,
        now,
    )?;
    persist_engine_state(config, &engine)?;

    Ok(DaemonOutput {
        socket_path: config.completion.daemon_socket_path.clone(),
        received_events: events.len(),
        processed_events,
        reconciled_events,
        leak_candidates: leak_output.leak_candidates,
        leak_process_remediations: leak_output.leak_process_remediations,
        leak_group_remediations: leak_output.leak_group_remediations,
        rust_analyzer_memory_candidates: memory_guard_output.rust_analyzer_memory_candidates,
        rust_analyzer_memory_remediations: memory_guard_output.rust_analyzer_memory_remediations,
    })
}

pub async fn run_daemon_loop(
    config: &GuardianConfig,
    options: DaemonRunOptions,
) -> Result<DaemonOutput, IpcError> {
    let ownership_policy = build_ownership_policy(config);
    let term_timeout = Duration::from_secs(config.completion.cleanup_retry_interval_secs.max(1));
    let mut system = sysinfo::System::new();
    let mut snapshot_cache = StorageSnapshotCache::new(&config.storage.allowlist)?;
    let mut engine = build_cleanup_engine_with_resolver(config, CandidateResolver::default())?;
    let mut detector = LeakDetector::default();
    let mut rust_analyzer_guard = RustAnalyzerMemoryGuard::default();
    let listener = CompletionEventListener::bind(&config.completion.daemon_socket_path)?;
    let mut output = build_daemon_output(config.completion.daemon_socket_path.clone());
    let mut cycles = 0_usize;

    loop {
        let events = listener
            .receive_batch(options.max_events_per_batch, Some(options.idle_timeout))
            .await?;

        let inventory = ProcessInventory::collect_live_with(&mut system);
        let snapshot = snapshot_cache.refresh_if_needed()?;
        engine.set_resolver(build_resolver_from(&inventory, snapshot));

        output.received_events += events.len();
        let now = SystemTime::now();
        output.processed_events += process_event_batch(config, &mut engine, &events)?;
        output.reconciled_events += run_reconciliation_cycle(config, &mut engine, snapshot, now)?;
        let leak_output = run_leak_enforcement_with_inventory(
            config,
            &mut detector,
            &inventory,
            &ownership_policy,
            term_timeout,
        )?;
        let memory_guard_output = run_rust_analyzer_memory_guard_with_inventory(
            config,
            &mut rust_analyzer_guard,
            &inventory,
            term_timeout,
            now,
        )?;
        output.leak_candidates += leak_output.leak_candidates;
        output.leak_process_remediations += leak_output.leak_process_remediations;
        output.leak_group_remediations += leak_output.leak_group_remediations;
        output.rust_analyzer_memory_candidates +=
            memory_guard_output.rust_analyzer_memory_candidates;
        output.rust_analyzer_memory_remediations +=
            memory_guard_output.rust_analyzer_memory_remediations;
        persist_engine_state(config, &engine)?;

        cycles += 1;
        if options.max_cycles.is_some_and(|limit| cycles >= limit) {
            break;
        }
    }

    Ok(output)
}

#[cfg(test)]
fn build_cleanup_engine(config: &GuardianConfig) -> Result<AutoCleanupEngine, IpcError> {
    let resolver = build_resolver(config)?;
    build_cleanup_engine_with_resolver(config, resolver)
}

fn build_cleanup_engine_with_resolver(
    config: &GuardianConfig,
    resolver: CandidateResolver,
) -> Result<AutoCleanupEngine, IpcError> {
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

#[cfg(test)]
fn build_resolver(config: &GuardianConfig) -> Result<CandidateResolver, IpcError> {
    let snapshot = scan_allowlisted_roots(&config.storage.allowlist).map_err(execution_error)?;
    let inventory = ProcessInventory::collect_live();
    Ok(build_resolver_from(&inventory, &snapshot))
}

fn build_resolver_from(
    inventory: &ProcessInventory,
    snapshot: &StorageSnapshot,
) -> CandidateResolver {
    CandidateResolver::new(
        SessionProcessIndex::from_inventory(inventory),
        SessionArtifactIndex::from_snapshot(snapshot),
        SessionPortIndex::from_inventory(inventory),
    )
}

fn process_event_batch(
    config: &GuardianConfig,
    engine: &mut AutoCleanupEngine,
    events: &[CompletionEvent],
) -> Result<usize, IpcError> {
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
    snapshot: &StorageSnapshot,
    now: SystemTime,
) -> Result<usize, IpcError> {
    if !config
        .completion
        .enabled_sources
        .contains(&CompletionSource::Inferred)
    {
        return Ok(0);
    }

    engine
        .run_reconciliation_pass_with_snapshot(snapshot, now)
        .map(|outcomes| outcomes.len())
        .map_err(execution_error)
}

fn persist_engine_state(
    config: &GuardianConfig,
    engine: &AutoCleanupEngine,
) -> Result<(), IpcError> {
    let now_unix_secs = SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let mut state = CompletionStateStore::from_snapshot(
        config.completion.dedupe_ttl_secs,
        engine.state_snapshot(),
    );
    state.purge_expired(now_unix_secs);
    persist_completion_state(&config.completion.state_path, &state).map_err(execution_error)
}

#[cfg(test)]
mod tests {
    use std::fs;
    #[cfg(any(unix, windows))]
    use std::process::Command;
    use std::sync::mpsc;
    use std::time::{Duration, UNIX_EPOCH};

    use notify::event::{DataChange, ModifyKind};
    use notify::{Event, EventKind};
    use tempfile::tempdir;

    #[cfg(unix)]
    use super::run_rust_analyzer_memory_guard_with_inventory;
    use super::{
        DaemonRunOptions, StorageSnapshotCache, build_cleanup_engine, build_cleanup_settings,
        build_daemon_output, build_ownership_policy, build_resolver, execution_error,
        process_event_batch, remediation_succeeded, run_leak_enforcement_with_inventory,
        run_reconciliation_cycle, run_rust_analyzer_memory_guard_once,
        run_rust_analyzer_memory_guard_once_with_inventory,
    };
    use crate::completion::{
        CompletionEvent, CompletionRecordState, CompletionSource, CompletionStateEntry,
        CompletionStateSnapshot,
    };
    use crate::config::{
        CompletionCleanupPolicy, GuardianConfig, LeakDetectionPolicy, Mode,
        RustAnalyzerMemoryGuardPolicy, SafetyPolicy, SamplingPolicy, StoragePolicy,
    };
    use crate::leak::LeakDetector;
    use crate::memory_guard::RustAnalyzerMemoryGuard;
    use crate::monitor::process::{ProcessInventory, ProcessSample};
    use crate::monitor::storage::{StorageSnapshot, scan_allowlisted_roots};
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

    fn watch_event(kind: EventKind, paths: Vec<std::path::PathBuf>) -> Event {
        Event {
            kind,
            paths,
            attrs: Default::default(),
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
        assert_eq!(output.rust_analyzer_memory_candidates, 0);
        assert_eq!(output.rust_analyzer_memory_remediations, 0);
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
        let reconciled = run_reconciliation_cycle(
            &config,
            &mut engine,
            &StorageSnapshot::default(),
            UNIX_EPOCH,
        )
        .expect("reconciliation should skip cleanly");

        assert_eq!(reconciled, 0);
    }

    #[test]
    fn execution_error_wraps_displayable_sources() {
        let error = execution_error("boom");

        assert_eq!(error.to_string(), "execution_failed: boom");
    }

    #[test]
    fn storage_snapshot_cache_applies_incremental_file_updates() {
        let dir = tempdir().expect("tempdir");
        let artifact = dir.path().join("artifact.json");
        fs::write(&artifact, "abc").expect("artifact should exist");
        let (tx, rx) = mpsc::channel();
        let mut cache = StorageSnapshotCache {
            allowlisted_roots: vec![dir.path().to_path_buf()],
            snapshot: scan_allowlisted_roots(&[dir.path().to_path_buf()]).expect("scan"),
            _watcher: None,
            watch_events: Some(rx),
            snapshot_used: true,
            fallback_rescan_each_cycle: false,
        };

        fs::write(&artifact, "abcdef").expect("artifact should be updated");
        tx.send(Ok(watch_event(
            EventKind::Modify(ModifyKind::Data(DataChange::Content)),
            vec![artifact.clone()],
        )))
        .expect("event should send");

        let snapshot = cache.refresh_if_needed().expect("refresh should succeed");

        assert_eq!(snapshot.total_bytes, 6);
        assert_eq!(snapshot.artifacts.len(), 1);
        assert_eq!(snapshot.artifacts[0].path, artifact);
        assert_eq!(snapshot.artifacts[0].bytes, 6);
    }

    #[test]
    fn storage_snapshot_cache_falls_back_to_full_rescan_for_ambiguous_events() {
        let dir = tempdir().expect("tempdir");
        let artifact = dir.path().join("artifact.json");
        fs::write(&artifact, "abc").expect("artifact should exist");
        let (tx, rx) = mpsc::channel();
        let mut cache = StorageSnapshotCache {
            allowlisted_roots: vec![dir.path().to_path_buf()],
            snapshot: scan_allowlisted_roots(&[dir.path().to_path_buf()]).expect("scan"),
            _watcher: None,
            watch_events: Some(rx),
            snapshot_used: true,
            fallback_rescan_each_cycle: false,
        };

        fs::write(&artifact, "abcdef").expect("artifact should be updated");
        tx.send(Ok(watch_event(EventKind::Any, vec![artifact.clone()])))
            .expect("event should send");

        let snapshot = cache.refresh_if_needed().expect("refresh should succeed");

        assert_eq!(snapshot.artifacts.len(), 1);
        assert_eq!(snapshot.artifacts[0].path, artifact);
        assert_eq!(snapshot.artifacts[0].bytes, 6);
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

    fn rust_analyzer_test_inventory(
        pid: u32,
        start_time_secs: u64,
        memory_bytes: u64,
    ) -> ProcessInventory {
        ProcessInventory::from_samples([ProcessSample {
            pid,
            parent_pid: Some(1),
            pgid: None,
            start_time_secs,
            uid: Some(current_effective_uid()),
            memory_bytes,
            cpu_percent: 0.2,
            command: "rust-analyzer --stdio".to_string(),
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
        let ownership = build_ownership_policy(&config);
        let term_timeout = Duration::from_secs(1);
        let mut detector = LeakDetector::default();

        assert_eq!(
            run_leak_enforcement_with_inventory(
                &config,
                &mut detector,
                &leak_test_inventory(77, 42, 100),
                &ownership,
                term_timeout,
            )
            .expect("first sample"),
            super::LeakCleanupOutput::default()
        );
        assert_eq!(
            run_leak_enforcement_with_inventory(
                &config,
                &mut detector,
                &leak_test_inventory(77, 42, 130),
                &ownership,
                term_timeout,
            )
            .expect("second sample"),
            super::LeakCleanupOutput::default()
        );

        let output = run_leak_enforcement_with_inventory(
            &config,
            &mut detector,
            &leak_test_inventory(77, 42, 160),
            &ownership,
            term_timeout,
        )
        .expect("third sample");

        assert_eq!(output.leak_candidates, 1);
        assert_eq!(output.leak_process_remediations, 0);
        assert_eq!(output.leak_group_remediations, 0);
    }

    #[test]
    fn run_rust_analyzer_memory_guard_once_boundary_reports_candidates_in_observe_mode() {
        let config = GuardianConfig {
            mode: Mode::Observe,
            rust_analyzer_memory_guard: RustAnalyzerMemoryGuardPolicy {
                enabled: true,
                max_rss_bytes: 100,
                required_consecutive_samples: 2,
                startup_grace_secs: 0,
                cooldown_secs: 300,
                same_uid_only: true,
            },
            completion: CompletionCleanupPolicy {
                cleanup_retry_interval_secs: 1,
                ..CompletionCleanupPolicy::default()
            },
            ..GuardianConfig::default()
        };
        let now = UNIX_EPOCH + Duration::from_secs(500);
        let mut guard = RustAnalyzerMemoryGuard::default();

        assert_eq!(
            run_rust_analyzer_memory_guard_once_with_inventory(
                &config,
                &mut guard,
                &rust_analyzer_test_inventory(77, 42, 110),
                now,
            )
            .expect("first sample"),
            super::MemoryGuardOutput::default()
        );

        let output = run_rust_analyzer_memory_guard_once_with_inventory(
            &config,
            &mut guard,
            &rust_analyzer_test_inventory(77, 42, 120),
            now,
        )
        .expect("second sample");

        assert_eq!(output.rust_analyzer_memory_candidates, 1);
        assert_eq!(output.rust_analyzer_memory_remediations, 0);
    }

    #[test]
    fn run_rust_analyzer_memory_guard_once_returns_zero_when_guard_disabled() {
        let config = GuardianConfig {
            rust_analyzer_memory_guard: RustAnalyzerMemoryGuardPolicy {
                enabled: false,
                ..RustAnalyzerMemoryGuardPolicy::default()
            },
            ..GuardianConfig::default()
        };

        let output = run_rust_analyzer_memory_guard_once(&config)
            .expect("boundary rust-analyzer guard should execute");

        assert_eq!(output, super::MemoryGuardOutput::default());
    }

    #[cfg(unix)]
    #[test]
    fn run_rust_analyzer_memory_guard_terminates_process_in_enforce_mode() {
        let mut child = Command::new("sleep")
            .arg("30")
            .spawn()
            .expect("sleep process should spawn");

        let config = GuardianConfig {
            mode: Mode::Enforce,
            rust_analyzer_memory_guard: RustAnalyzerMemoryGuardPolicy {
                enabled: true,
                max_rss_bytes: 100,
                required_consecutive_samples: 2,
                startup_grace_secs: 0,
                cooldown_secs: 300,
                same_uid_only: true,
            },
            completion: CompletionCleanupPolicy {
                cleanup_retry_interval_secs: 1,
                ..CompletionCleanupPolicy::default()
            },
            ..GuardianConfig::default()
        };
        let term_timeout = Duration::from_secs(1);
        let now = UNIX_EPOCH + Duration::from_secs(500);
        let mut guard = RustAnalyzerMemoryGuard::default();

        for memory_bytes in [110_u64, 120] {
            let _ = run_rust_analyzer_memory_guard_with_inventory(
                &config,
                &mut guard,
                &rust_analyzer_test_inventory(child.id(), 42, memory_bytes),
                term_timeout,
                now,
            )
            .expect("memory guard should run");
        }

        let status = child.wait().expect("child should exit after remediation");
        assert!(!status.success());
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
        let ownership = build_ownership_policy(&config);
        let term_timeout = Duration::from_secs(1);
        let mut detector = LeakDetector::default();

        for memory_bytes in [100_u64, 130, 160] {
            let _ = run_leak_enforcement_with_inventory(
                &config,
                &mut detector,
                &leak_test_inventory(child.id(), 42, memory_bytes),
                &ownership,
                term_timeout,
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
        let ownership = build_ownership_policy(&config);
        let term_timeout = Duration::from_secs(1);
        let mut detector = LeakDetector::default();

        for memory_bytes in [100_u64, 130, 160] {
            let _ = run_leak_enforcement_with_inventory(
                &config,
                &mut detector,
                &leak_test_inventory(child.id(), 42, memory_bytes),
                &ownership,
                term_timeout,
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
