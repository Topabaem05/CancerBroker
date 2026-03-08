use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::completion::{CompletionEvent, CompletionSource};
use crate::monitor::process::ProcessInventory;
use crate::monitor::storage::StorageSnapshot;
use crate::safety::ProcessIdentity;

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
                    .push(ProcessIdentity {
                        pid: sample.pid,
                        parent_pid: sample.parent_pid,
                        start_time_secs: sample.start_time_secs,
                        uid: sample.uid,
                        command: sample.command.clone(),
                    });
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

        if event.source == CompletionSource::ToolPartCompleted
            && event.tool_name.as_deref() != Some("task")
        {
            return ResolvedCandidates::default();
        }

        let processes = self.process_index.resolve(session_id);
        let artifacts = self.artifact_index.resolve(session_id);

        ResolvedCandidates {
            immediate_cleanup_eligible: !processes.is_empty() || !artifacts.is_empty(),
            deferred_to_reconciliation: false,
            processes,
            artifacts,
        }
    }
}

pub(crate) fn session_ids_in_text(text: &str) -> Vec<String> {
    text.split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
        .filter(|part| part.starts_with("ses_") && part.len() > 4)
        .map(ToOwned::to_owned)
        .collect()
}
