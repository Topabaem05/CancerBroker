use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use thiserror::Error;
#[cfg(unix)]
use tokio::io::AsyncWriteExt;

use crate::cleanup::{CleanupOutcome, CleanupPolicy, remove_stale_allowlisted_artifacts};
use crate::completion::{CompletionEvent, CompletionStateSnapshot};
use crate::dispatch::{CleanupDispatcher, DispatchDecision};
use crate::ipc::{IpcError, receive_completion_events_once};
use crate::monitor::resources::{ProcessResourceReport, collect_process_resources};
use crate::monitor::storage::{StorageSnapshot, scan_allowlisted_roots};
use crate::remediation::{
    ProcessGroupRemediationRequest, ProcessRemediationOutcome, ProcessRemediationRequest,
    RemediationError, remediate_process, remediate_process_group,
};
use crate::resolution::session_ids_in_text;
use crate::resolution::{CandidateResolver, ResolvedCandidates};
use crate::safety::OwnershipPolicy;

#[derive(Debug, Clone)]
pub struct AutoCleanupSettings {
    pub cleanup_policy: CleanupPolicy,
    pub ownership_policy: OwnershipPolicy,
    pub term_timeout: Duration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AutoCleanupDecision {
    ProcessedNow,
    DeferredToReconciliation,
    SkippedDuplicate,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessCleanupResult {
    pub pid: u32,
    pub resources: ProcessResourceReport,
    pub outcome: ProcessRemediationOutcome,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessGroupCleanupResult {
    pub pgid: u32,
    pub outcome: ProcessRemediationOutcome,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutoCleanupResult {
    pub decision: AutoCleanupDecision,
    pub cleanup_outcome: CleanupOutcome,
    pub process_outcomes: Vec<ProcessCleanupResult>,
    pub group_outcomes: Vec<ProcessGroupCleanupResult>,
}

#[derive(Debug, Clone)]
pub struct AutoCleanupEngine {
    dispatcher: CleanupDispatcher,
    settings: AutoCleanupSettings,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DaemonCleanupOutput {
    pub processed_events: usize,
    pub reconciled_events: usize,
}

#[derive(Debug, Error)]
pub enum AutoCleanupError {
    #[error("cleanup_failed: {0}")]
    Cleanup(#[from] std::io::Error),
    #[error("remediation_failed: {0}")]
    Remediation(#[from] RemediationError),
    #[error("ipc_failed: {0}")]
    Ipc(#[from] IpcError),
}

impl AutoCleanupEngine {
    pub fn new(dispatcher: CleanupDispatcher, settings: AutoCleanupSettings) -> Self {
        Self {
            dispatcher,
            settings,
        }
    }

    pub fn handle_completion_event(
        &mut self,
        event: &CompletionEvent,
        now: SystemTime,
    ) -> Result<AutoCleanupResult, AutoCleanupError> {
        let now_unix_secs = unix_timestamp_secs(now);

        match self.dispatcher.dispatch(event, now_unix_secs) {
            DispatchDecision::SkipDuplicate => Ok(AutoCleanupResult {
                decision: AutoCleanupDecision::SkippedDuplicate,
                cleanup_outcome: CleanupOutcome::default(),
                process_outcomes: Vec::new(),
                group_outcomes: Vec::new(),
            }),
            DispatchDecision::DeferredToReconciliation(_) => Ok(AutoCleanupResult {
                decision: AutoCleanupDecision::DeferredToReconciliation,
                cleanup_outcome: CleanupOutcome::default(),
                process_outcomes: Vec::new(),
                group_outcomes: Vec::new(),
            }),
            DispatchDecision::Immediate(resolved) => {
                let result = self.execute_cleanup(event, resolved, now)?;

                if result.decision == AutoCleanupDecision::ProcessedNow {
                    self.dispatcher.mark_processed(event, now_unix_secs);
                }

                Ok(result)
            }
        }
    }

    pub fn run_reconciliation_pass(
        &mut self,
        now: SystemTime,
    ) -> Result<Vec<AutoCleanupResult>, AutoCleanupError> {
        let events = infer_reconciliation_events(&self.settings.cleanup_policy.allowlist, now)?;
        run_reconciliation(self, &events, now)
    }

    pub fn state_snapshot(&self) -> CompletionStateSnapshot {
        self.dispatcher.snapshot()
    }

    pub fn set_resolver(&mut self, resolver: CandidateResolver) {
        self.dispatcher.set_resolver(resolver);
    }
}

pub fn run_reconciliation(
    engine: &mut AutoCleanupEngine,
    events: &[CompletionEvent],
    now: SystemTime,
) -> Result<Vec<AutoCleanupResult>, AutoCleanupError> {
    let mut outcomes = Vec::with_capacity(events.len());

    for event in events {
        outcomes.push(engine.handle_completion_event(event, now)?);
    }

    Ok(outcomes)
}

#[cfg(unix)]
pub async fn run_daemon_once_with_cleanup(
    socket_path: &Path,
    engine: &mut AutoCleanupEngine,
    max_events: usize,
    payload: &[u8],
) -> Result<DaemonCleanupOutput, AutoCleanupError> {
    let socket_path_buf = socket_path.to_path_buf();
    let payload_bytes = payload.to_vec();
    let writer = tokio::spawn(async move {
        for _ in 0..50 {
            match tokio::net::UnixStream::connect(&socket_path_buf).await {
                Ok(mut stream) => {
                    stream.write_all(&payload_bytes).await?;
                    return Ok::<(), std::io::Error>(());
                }
                Err(_) => tokio::time::sleep(Duration::from_millis(10)).await,
            }
        }

        Err(std::io::Error::other("failed to connect to daemon socket"))
    });

    let events = receive_completion_events_once(socket_path, max_events).await?;
    writer
        .await
        .map_err(|error| std::io::Error::other(error.to_string()))??;

    for event in &events {
        engine.handle_completion_event(event, SystemTime::now())?;
    }

    let reconciled_events = engine.run_reconciliation_pass(SystemTime::now())?.len();

    Ok(DaemonCleanupOutput {
        processed_events: events.len(),
        reconciled_events,
    })
}

#[cfg(not(unix))]
pub async fn run_daemon_once_with_cleanup(
    _socket_path: &Path,
    _engine: &mut AutoCleanupEngine,
    _max_events: usize,
    _payload: &[u8],
) -> Result<DaemonCleanupOutput, AutoCleanupError> {
    Err(IpcError::UnsupportedPlatform.into())
}

pub fn infer_reconciliation_events(
    allowlisted_roots: &[PathBuf],
    now: SystemTime,
) -> Result<Vec<CompletionEvent>, std::io::Error> {
    let snapshot = scan_allowlisted_roots(allowlisted_roots)?;
    let session_ids = collect_session_ids_from_snapshot(&snapshot);

    Ok(session_ids
        .into_iter()
        .map(|session_id| build_inferred_completion_event(session_id, now))
        .collect())
}

impl AutoCleanupEngine {
    fn execute_cleanup(
        &self,
        _event: &CompletionEvent,
        resolved: ResolvedCandidates,
        now: SystemTime,
    ) -> Result<AutoCleanupResult, AutoCleanupError> {
        let (process_outcomes, group_outcomes) = self.remediate_processes(&resolved)?;
        let cleanup_outcome = remove_stale_allowlisted_artifacts(
            &resolved.artifacts,
            &self.settings.cleanup_policy,
            now,
        )?;

        let decision = if !cleanup_outcome.skipped_active_session_grace.is_empty() {
            AutoCleanupDecision::DeferredToReconciliation
        } else {
            AutoCleanupDecision::ProcessedNow
        };

        Ok(AutoCleanupResult {
            decision,
            cleanup_outcome,
            process_outcomes,
            group_outcomes,
        })
    }

    fn remediate_processes(
        &self,
        resolved: &ResolvedCandidates,
    ) -> Result<(Vec<ProcessCleanupResult>, Vec<ProcessGroupCleanupResult>), AutoCleanupError> {
        let mut process_outcomes = Vec::with_capacity(resolved.processes.len());
        let mut seen_pgids = std::collections::BTreeSet::new();
        let mut group_leaders: Vec<(u32, crate::safety::ProcessIdentity)> = Vec::new();

        for identity in &resolved.processes {
            let outcome = remediate_process(&ProcessRemediationRequest {
                identity: identity.clone(),
                ownership_policy: self.settings.ownership_policy.clone(),
                term_timeout: self.settings.term_timeout,
            })?;

            if outcome != ProcessRemediationOutcome::Rejected("uid_mismatch")
                && outcome != ProcessRemediationOutcome::Rejected("missing_command_marker")
                && let Some(pgid) = identity.pgid
                && seen_pgids.insert(pgid)
            {
                group_leaders.push((pgid, identity.clone()));
            }

            process_outcomes.push(ProcessCleanupResult {
                pid: identity.pid,
                resources: collect_process_resources(identity.pid),
                outcome,
            });
        }

        let mut group_outcomes = Vec::with_capacity(group_leaders.len());
        for (pgid, leader) in group_leaders {
            let outcome = remediate_process_group(&ProcessGroupRemediationRequest {
                pgid,
                leader_identity: leader,
                ownership_policy: self.settings.ownership_policy.clone(),
                term_timeout: self.settings.term_timeout,
            })?;
            group_outcomes.push(ProcessGroupCleanupResult { pgid, outcome });
        }

        Ok((process_outcomes, group_outcomes))
    }
}

fn unix_timestamp_secs(now: SystemTime) -> u64 {
    now.duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::from_secs(0))
        .as_secs()
}

fn collect_session_ids_from_snapshot(snapshot: &StorageSnapshot) -> BTreeSet<String> {
    let mut session_ids = BTreeSet::new();

    for artifact in &snapshot.artifacts {
        let text = artifact.path.to_string_lossy();
        for session_id in session_ids_in_text(&text) {
            session_ids.insert(session_id);
        }
    }

    session_ids
}

fn build_inferred_completion_event(session_id: String, now: SystemTime) -> CompletionEvent {
    CompletionEvent {
        event_id: format!("inferred:{session_id}"),
        session_id: Some(session_id),
        parent_session_id: None,
        task_id: None,
        tool_name: None,
        completed_at: unix_timestamp_secs(now).to_string(),
        source: crate::completion::CompletionSource::Inferred,
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{Duration, UNIX_EPOCH};

    use tempfile::tempdir;

    use super::{
        AutoCleanupEngine, AutoCleanupSettings, infer_reconciliation_events, unix_timestamp_secs,
    };
    use crate::cleanup::CleanupPolicy;
    use crate::completion::CompletionSource;
    use crate::completion::CompletionStateStore;
    use crate::dispatch::CleanupDispatcher;
    use crate::resolution::ResolvedCandidates;
    use crate::safety::{OwnershipPolicy, ProcessIdentity};

    fn sample_settings() -> AutoCleanupSettings {
        AutoCleanupSettings {
            cleanup_policy: CleanupPolicy {
                allowlist: Vec::new(),
                active_session_grace: Duration::from_secs(60),
            },
            ownership_policy: OwnershipPolicy {
                expected_uid: u32::MAX,
                required_command_markers: vec!["opencode".to_string()],
                same_uid_only: true,
            },
            term_timeout: Duration::from_millis(1),
        }
    }

    #[test]
    fn infer_reconciliation_events_collects_unique_session_ids() {
        let dir = tempdir().expect("tempdir");
        let session_dir = dir.path().join("ses_alpha");
        fs::create_dir_all(&session_dir).expect("session dir should exist");
        fs::write(session_dir.join("artifact.json"), "{}").expect("artifact should be written");
        fs::write(
            dir.path().join("ses_beta_report.txt"),
            "artifact for ses_beta",
        )
        .expect("second artifact should be written");
        fs::write(session_dir.join("artifact-2.log"), "dup")
            .expect("duplicate artifact should be written");

        let events = infer_reconciliation_events(&[dir.path().to_path_buf()], UNIX_EPOCH)
            .expect("reconciliation events should be inferred");

        assert_eq!(events.len(), 2);
        assert_eq!(events[0].event_id, "inferred:ses_alpha");
        assert_eq!(events[0].session_id.as_deref(), Some("ses_alpha"));
        assert_eq!(events[0].completed_at, "0");
        assert_eq!(events[0].source, CompletionSource::Inferred);
        assert_eq!(events[1].event_id, "inferred:ses_beta_report");
    }

    #[test]
    fn infer_reconciliation_events_ignores_non_session_artifacts() {
        let dir = tempdir().expect("tempdir");
        fs::write(dir.path().join("artifact.json"), "{}").expect("artifact should be written");

        let events = infer_reconciliation_events(&[dir.path().to_path_buf()], UNIX_EPOCH)
            .expect("non-session artifacts should not fail");

        assert!(events.is_empty());
    }

    #[test]
    fn unix_timestamp_secs_clamps_pre_epoch_times() {
        let before_epoch = UNIX_EPOCH
            .checked_sub(Duration::from_secs(1))
            .expect("pre-epoch time should exist");

        assert_eq!(unix_timestamp_secs(before_epoch), 0);
    }

    #[test]
    fn execute_cleanup_captures_open_resources_before_process_outcome() {
        let engine = AutoCleanupEngine::new(
            CleanupDispatcher::new(CompletionStateStore::new(60), Default::default()),
            sample_settings(),
        );

        let result = engine
            .execute_cleanup(
                &crate::completion::CompletionEvent {
                    event_id: "evt-1".to_string(),
                    session_id: Some("ses_alpha".to_string()),
                    parent_session_id: None,
                    task_id: None,
                    tool_name: None,
                    completed_at: "0".to_string(),
                    source: CompletionSource::Status,
                },
                ResolvedCandidates {
                    processes: vec![ProcessIdentity {
                        pid: std::process::id(),
                        parent_pid: None,
                        pgid: None,
                        start_time_secs: 0,
                        uid: Some(0),
                        command: "python worker".to_string(),
                        listening_ports: vec![],
                    }],
                    artifacts: Vec::new(),
                    immediate_cleanup_eligible: true,
                    deferred_to_reconciliation: false,
                },
                UNIX_EPOCH,
            )
            .expect("cleanup result");

        assert_eq!(result.process_outcomes.len(), 1);
        assert_eq!(result.process_outcomes[0].pid, std::process::id());
        assert_eq!(result.process_outcomes[0].resources.pid, std::process::id());
        assert_eq!(
            result.process_outcomes[0].outcome,
            crate::remediation::ProcessRemediationOutcome::Rejected("uid_mismatch")
        );
    }
}
