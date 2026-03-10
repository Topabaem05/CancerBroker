use crate::completion::{
    CompletionEvent, CompletionStateSnapshot, CompletionStateStore, CompletionStoreBegin,
};
use crate::resolution::{CandidateResolver, ResolvedCandidates};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DispatchDecision {
    Immediate(ResolvedCandidates),
    DeferredToReconciliation(ResolvedCandidates),
    SkipDuplicate,
}

#[derive(Debug, Clone)]
pub struct CleanupDispatcher {
    state: CompletionStateStore,
    resolver: CandidateResolver,
}

fn build_dispatch_decision(resolved: ResolvedCandidates) -> DispatchDecision {
    if resolved.immediate_cleanup_eligible {
        DispatchDecision::Immediate(resolved)
    } else {
        DispatchDecision::DeferredToReconciliation(resolved)
    }
}

impl CleanupDispatcher {
    pub fn new(state: CompletionStateStore, resolver: CandidateResolver) -> Self {
        Self { state, resolver }
    }

    pub fn dispatch(&mut self, event: &CompletionEvent, now_unix_secs: u64) -> DispatchDecision {
        match self.state.begin(event, now_unix_secs) {
            CompletionStoreBegin::SkipDuplicate => DispatchDecision::SkipDuplicate,
            CompletionStoreBegin::Accepted | CompletionStoreBegin::RetryPending => {
                build_dispatch_decision(self.resolver.resolve(event))
            }
        }
    }

    pub fn mark_processed(&mut self, event: &CompletionEvent, now_unix_secs: u64) {
        self.state.mark_processed(event, now_unix_secs);
    }

    pub fn pending_keys(&self) -> Vec<String> {
        self.state.pending_keys()
    }

    pub fn snapshot(&self) -> CompletionStateSnapshot {
        self.state.snapshot()
    }

    pub fn set_resolver(&mut self, resolver: CandidateResolver) {
        self.resolver = resolver;
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::UNIX_EPOCH;

    use super::{CleanupDispatcher, DispatchDecision, build_dispatch_decision};
    use crate::completion::{CompletionEvent, CompletionSource, CompletionStateStore};
    use crate::monitor::storage::{ArtifactRecord, StorageSnapshot};
    use crate::resolution::{
        CandidateResolver, ResolvedCandidates, SessionArtifactIndex, SessionPortIndex,
        SessionProcessIndex,
    };

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
    fn build_dispatch_decision_returns_immediate_for_cleanup_candidates() {
        let decision = build_dispatch_decision(ResolvedCandidates {
            immediate_cleanup_eligible: true,
            artifacts: vec!["/tmp/ses_alpha.json".into()],
            ..ResolvedCandidates::default()
        });

        assert!(matches!(decision, DispatchDecision::Immediate(_)));
    }

    #[test]
    fn build_dispatch_decision_returns_deferred_without_candidates() {
        let decision = build_dispatch_decision(ResolvedCandidates::default());

        assert!(matches!(
            decision,
            DispatchDecision::DeferredToReconciliation(_)
        ));
    }

    #[test]
    fn dispatch_skips_processed_duplicates_within_ttl() {
        let resolver = CandidateResolver::new(
            SessionProcessIndex::default(),
            SessionArtifactIndex::default(),
            SessionPortIndex::default(),
        );
        let event = status_event(Some("ses_alpha"));
        let mut dispatcher = CleanupDispatcher::new(CompletionStateStore::new(60), resolver);

        assert!(matches!(
            dispatcher.dispatch(&event, 10),
            DispatchDecision::DeferredToReconciliation(_)
        ));
        dispatcher.mark_processed(&event, 20);

        assert!(matches!(
            dispatcher.dispatch(&event, 70),
            DispatchDecision::SkipDuplicate
        ));
    }

    #[test]
    fn dispatch_retries_pending_entries_without_losing_pending_state() {
        let resolver = CandidateResolver::new(
            SessionProcessIndex::default(),
            SessionArtifactIndex::default(),
            SessionPortIndex::default(),
        );
        let event = status_event(Some("ses_alpha"));
        let mut dispatcher = CleanupDispatcher::new(CompletionStateStore::new(60), resolver);

        assert!(matches!(
            dispatcher.dispatch(&event, 10),
            DispatchDecision::DeferredToReconciliation(_)
        ));
        assert!(matches!(
            dispatcher.dispatch(&event, 20),
            DispatchDecision::DeferredToReconciliation(_)
        ));
        assert_eq!(dispatcher.pending_keys(), vec![event.dedupe_key()]);
    }

    #[test]
    fn dispatch_returns_immediate_when_artifact_candidates_exist() {
        let resolver = CandidateResolver::new(
            SessionProcessIndex::default(),
            SessionArtifactIndex::from_snapshot(&StorageSnapshot {
                artifacts: vec![ArtifactRecord {
                    path: "/tmp/ses_alpha_artifact.json".into(),
                    bytes: 2,
                    modified_at: UNIX_EPOCH,
                }],
                total_bytes: 2,
            }),
            SessionPortIndex::default(),
        );
        let event = status_event(Some("ses_alpha_artifact"));
        let mut dispatcher = CleanupDispatcher::new(CompletionStateStore::new(60), resolver);

        match dispatcher.dispatch(&event, 10) {
            DispatchDecision::Immediate(resolved) => {
                assert_eq!(
                    resolved.artifacts,
                    vec![PathBuf::from("/tmp/ses_alpha_artifact.json")]
                );
                assert!(resolved.immediate_cleanup_eligible);
            }
            _ => panic!("expected immediate dispatch decision"),
        }
    }

    #[test]
    fn dispatch_defers_when_event_has_no_session_id() {
        let resolver = CandidateResolver::new(
            SessionProcessIndex::default(),
            SessionArtifactIndex::default(),
            SessionPortIndex::default(),
        );
        let event = status_event(None);
        let mut dispatcher = CleanupDispatcher::new(CompletionStateStore::new(60), resolver);

        match dispatcher.dispatch(&event, 10) {
            DispatchDecision::DeferredToReconciliation(resolved) => {
                assert!(resolved.deferred_to_reconciliation);
                assert!(resolved.artifacts.is_empty());
                assert!(resolved.processes.is_empty());
            }
            _ => panic!("expected deferred dispatch decision"),
        }
    }
}
