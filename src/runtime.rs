use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::SystemTime;

use serde::Serialize;

use crate::config::GuardianConfig;
use crate::evidence::{
    EvidenceInput, EvidenceStore, SignalSnapshot, build_pre_action_evidence,
    persist_pre_action_with_fallback,
};
use crate::policy::{ActionHistoryRecord, DecisionInput, PolicyEngine, SignalWindow};

#[derive(Debug, Clone)]
pub struct RuntimeInput {
    pub target_id: String,
    pub signal_windows: Vec<SignalWindow>,
    pub history: Vec<ActionHistoryRecord>,
    pub now: SystemTime,
    pub evidence_dir: PathBuf,
}

#[derive(Debug, Clone, Serialize)]
pub struct RuntimeOutcome {
    pub proposed_action: Option<String>,
    pub executed_action: Option<String>,
    pub evidence_path: Option<PathBuf>,
    pub fallback_to_non_destructive: bool,
}

pub fn run_once(config: &GuardianConfig, input: RuntimeInput) -> RuntimeOutcome {
    let engine = PolicyEngine::new(config.sampling.clone(), config.budgets.clone());
    let decision = engine.decide(DecisionInput {
        mode: config.mode.clone(),
        target_id: input.target_id.clone(),
        signal_windows: input.signal_windows.clone(),
        history: input.history,
        now: input.now,
    });

    let mut evidence_path = None;
    let mut fallback_to_non_destructive = false;

    if let Some(proposed_stage) = &decision.proposed_stage {
        let evidence = build_pre_action_evidence(
            input.now,
            input.target_id,
            proposed_stage.as_str().to_string(),
            decision.rationale.clone(),
            input
                .signal_windows
                .iter()
                .map(|signal| SignalSnapshot {
                    name: signal.name.clone(),
                    breached_samples: signal.breached_samples,
                    window_samples: signal.window_samples,
                })
                .collect(),
            EvidenceInput {
                rationale: decision.rationale.clone(),
                prompt_excerpt: Some("runtime-context".to_string()),
                environment: BTreeMap::new(),
                metadata: BTreeMap::new(),
            },
        );

        let store = EvidenceStore::new(input.evidence_dir);
        let outcome = persist_pre_action_with_fallback(&store, &evidence);
        evidence_path = outcome.path;
        fallback_to_non_destructive = outcome.fallback_to_non_destructive;
    }

    RuntimeOutcome {
        proposed_action: decision
            .proposed_stage
            .map(|stage| stage.as_str().to_string()),
        executed_action: decision
            .executed_stage
            .map(|stage| stage.as_str().to_string()),
        evidence_path,
        fallback_to_non_destructive,
    }
}
