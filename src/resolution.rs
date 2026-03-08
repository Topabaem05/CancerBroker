use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::completion::{CompletionEvent, CompletionSource};
use crate::monitor::process::ProcessInventory;
use crate::monitor::storage::StorageSnapshot;
use crate::safety::ProcessIdentity;

fn build_process_identity(sample: &crate::monitor::process::ProcessSample) -> ProcessIdentity {
    ProcessIdentity {
        pid: sample.pid,
        parent_pid: sample.parent_pid,
        start_time_secs: sample.start_time_secs,
        uid: sample.uid,
        command: sample.command.clone(),
    }
}

fn build_resolved_candidates(
    processes: Vec<ProcessIdentity>,
    artifacts: Vec<PathBuf>,
) -> ResolvedCandidates {
    ResolvedCandidates {
        immediate_cleanup_eligible: !processes.is_empty() || !artifacts.is_empty(),
        deferred_to_reconciliation: false,
        processes,
        artifacts,
    }
}

fn accepts_completion_event(event: &CompletionEvent) -> bool {
    event.source != CompletionSource::ToolPartCompleted
        || event.tool_name.as_deref() == Some("task")
}

#[derive(Debug, Clone, Default)]
pub struct SessionProcessIndex {
    by_session_id: BTreeMap<String, Vec<ProcessIdentity>>,
}

#[derive(Debug, Clone, Default)]
pub struct SessionArtifactIndex {
    by_session_id: BTreeMap<String, Vec<PathBuf>>,
}

#[derive(Debug, Clone, Default)]
pub struct CandidateResolver {
    process_index: SessionProcessIndex,
    artifact_index: SessionArtifactIndex,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResolvedCandidates {
    pub processes: Vec<ProcessIdentity>,
    pub artifacts: Vec<PathBuf>,
    pub immediate_cleanup_eligible: bool,
    pub deferred_to_reconciliation: bool,
}

impl SessionProcessIndex {
    pub fn from_inventory(inventory: &ProcessInventory) -> Self {
        let mut by_session_id: BTreeMap<String, Vec<ProcessIdentity>> = BTreeMap::new();

        for sample in inventory.samples() {
            for session_id in session_ids_in_text(&sample.command) {
                by_session_id
                    .entry(session_id)
                    .or_default()
                    .push(build_process_identity(sample));
            }
        }

        Self { by_session_id }
    }

    pub fn resolve(&self, session_id: &str) -> Vec<ProcessIdentity> {
        self.by_session_id
            .get(session_id)
            .cloned()
            .unwrap_or_default()
    }
}

impl SessionArtifactIndex {
    pub fn from_snapshot(snapshot: &StorageSnapshot) -> Self {
        let mut by_session_id: BTreeMap<String, Vec<PathBuf>> = BTreeMap::new();

        for artifact in &snapshot.artifacts {
            let text = artifact.path.to_string_lossy();
            for session_id in session_ids_in_text(&text) {
                by_session_id
                    .entry(session_id)
                    .or_default()
                    .push(artifact.path.clone());
            }
        }

        Self { by_session_id }
    }

    pub fn resolve(&self, session_id: &str) -> Vec<PathBuf> {
        self.by_session_id
            .get(session_id)
            .cloned()
            .unwrap_or_default()
    }
}

impl CandidateResolver {
    pub fn new(process_index: SessionProcessIndex, artifact_index: SessionArtifactIndex) -> Self {
        Self {
            process_index,
            artifact_index,
        }
    }

    pub fn resolve(&self, event: &CompletionEvent) -> ResolvedCandidates {
        let Some(session_id) = event.session_id.as_deref() else {
            return ResolvedCandidates {
                deferred_to_reconciliation: true,
                ..ResolvedCandidates::default()
            };
        };

        if !accepts_completion_event(event) {
            return ResolvedCandidates::default();
        }

        let processes = self.process_index.resolve(session_id);
        let artifacts = self.artifact_index.resolve(session_id);

        build_resolved_candidates(processes, artifacts)
    }
}

pub(crate) fn session_ids_in_text(text: &str) -> Vec<String> {
    text.split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
        .filter(|part| part.starts_with("ses_") && part.len() > 4)
        .map(ToOwned::to_owned)
        .collect()
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::UNIX_EPOCH;

    use super::{
        CandidateResolver, SessionArtifactIndex, SessionProcessIndex, accepts_completion_event,
        build_resolved_candidates, session_ids_in_text,
    };
    use crate::completion::{CompletionEvent, CompletionSource};
    use crate::monitor::process::{ProcessInventory, ProcessSample};
    use crate::monitor::storage::{ArtifactRecord, StorageSnapshot};

    fn status_event(session_id: Option<&str>) -> CompletionEvent {
        CompletionEvent {
            event_id: "evt-1".to_string(),
            session_id: session_id.map(str::to_string),
            parent_session_id: None,
            task_id: None,
            tool_name: None,
            completed_at: "2026-03-08T20:00:00Z".to_string(),
            source: CompletionSource::Status,
        }
    }

    #[test]
    fn session_ids_in_text_extracts_multiple_session_tokens() {
        let session_ids = session_ids_in_text("prefix/ses_alpha/log ses_beta-1 other ses_");

        assert_eq!(
            session_ids,
            vec!["ses_alpha".to_string(), "ses_beta".to_string()]
        );
    }

    #[test]
    fn accepts_completion_event_requires_task_tool_for_tool_part_updates() {
        let mut event = status_event(Some("ses_alpha"));
        event.source = CompletionSource::ToolPartCompleted;
        event.tool_name = Some("write".to_string());
        assert!(!accepts_completion_event(&event));

        event.tool_name = Some("task".to_string());
        assert!(accepts_completion_event(&event));
    }

    #[test]
    fn session_process_index_maps_session_ids_from_inventory() {
        let inventory = ProcessInventory::from_samples([ProcessSample {
            pid: 10,
            parent_pid: Some(1),
            start_time_secs: 42,
            uid: Some(1000),
            memory_bytes: 128,
            cpu_percent: 0.5,
            command: "opencode ses_alpha worker".to_string(),
        }]);

        let index = SessionProcessIndex::from_inventory(&inventory);
        let resolved = index.resolve("ses_alpha");

        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].pid, 10);
        assert_eq!(resolved[0].command, "opencode ses_alpha worker");
    }

    #[test]
    fn session_artifact_index_maps_session_ids_from_snapshot() {
        let index = SessionArtifactIndex::from_snapshot(&StorageSnapshot {
            artifacts: vec![ArtifactRecord {
                path: "/tmp/ses_alpha_artifact.json".into(),
                bytes: 2,
                modified_at: UNIX_EPOCH,
            }],
            total_bytes: 2,
        });

        let resolved = index.resolve("ses_alpha_artifact");

        assert_eq!(
            resolved,
            vec![PathBuf::from("/tmp/ses_alpha_artifact.json")]
        );
    }

    #[test]
    fn build_resolved_candidates_marks_immediate_cleanup_when_any_match_exists() {
        let resolved = build_resolved_candidates(vec![], vec!["/tmp/file.json".into()]);

        assert!(resolved.immediate_cleanup_eligible);
        assert!(!resolved.deferred_to_reconciliation);
    }

    #[test]
    fn candidate_resolver_defers_events_without_session_ids() {
        let resolver = CandidateResolver::new(
            SessionProcessIndex::default(),
            SessionArtifactIndex::default(),
        );

        let resolved = resolver.resolve(&status_event(None));

        assert!(resolved.deferred_to_reconciliation);
        assert!(!resolved.immediate_cleanup_eligible);
    }

    #[test]
    fn candidate_resolver_returns_empty_for_unsupported_tool_part_events() {
        let resolver = CandidateResolver::new(
            SessionProcessIndex::default(),
            SessionArtifactIndex::default(),
        );
        let mut event = status_event(Some("ses_alpha"));
        event.source = CompletionSource::ToolPartCompleted;
        event.tool_name = Some("write".to_string());

        let resolved = resolver.resolve(&event);

        assert_eq!(resolved, Default::default());
    }

    #[test]
    fn candidate_resolver_returns_processes_and_artifacts_for_matching_session() {
        let process_index =
            SessionProcessIndex::from_inventory(&ProcessInventory::from_samples([ProcessSample {
                pid: 10,
                parent_pid: Some(1),
                start_time_secs: 42,
                uid: Some(1000),
                memory_bytes: 128,
                cpu_percent: 0.5,
                command: "opencode ses_alpha worker".to_string(),
            }]));
        let artifact_index = SessionArtifactIndex::from_snapshot(&StorageSnapshot {
            artifacts: vec![ArtifactRecord {
                path: "/tmp/ses_alpha/artifact.json".into(),
                bytes: 2,
                modified_at: UNIX_EPOCH,
            }],
            total_bytes: 2,
        });
        let resolver = CandidateResolver::new(process_index, artifact_index);

        let resolved = resolver.resolve(&status_event(Some("ses_alpha")));

        assert_eq!(resolved.processes.len(), 1);
        assert_eq!(resolved.artifacts.len(), 1);
        assert!(resolved.immediate_cleanup_eligible);
    }
}
