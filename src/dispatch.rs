use crate::completion::{CompletionEvent, CompletionStateStore, CompletionStoreBegin};
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

impl CleanupDispatcher {
    pub fn new(state: CompletionStateStore, resolver: CandidateResolver) -> Self {
        Self { state, resolver }
    }

    pub fn dispatch(&mut self, event: &CompletionEvent, now_unix_secs: u64) -> DispatchDecision {
        match self.state.begin(event, now_unix_secs) {
            CompletionStoreBegin::SkipDuplicate => DispatchDecision::SkipDuplicate,
            CompletionStoreBegin::Accepted | CompletionStoreBegin::RetryPending => {
                let resolved = self.resolver.resolve(event);

                if resolved.immediate_cleanup_eligible {
                    DispatchDecision::Immediate(resolved)
                } else {
                    DispatchDecision::DeferredToReconciliation(resolved)
                }
            }
        }
    }

    pub fn mark_processed(&mut self, event: &CompletionEvent, now_unix_secs: u64) {
        self.state.mark_processed(event, now_unix_secs);
    }

    pub fn pending_keys(&self) -> Vec<String> {
        self.state.pending_keys()
    }
}
