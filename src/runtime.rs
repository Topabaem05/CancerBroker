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

fn build_decision_input(config: &GuardianConfig, input: &RuntimeInput) -> DecisionInput {
    DecisionInput {
        mode: config.mode.clone(),
        target_id: input.target_id.clone(),
        signal_windows: input.signal_windows.clone(),
        history: input.history.clone(),
        now: input.now,
    }
}

fn build_signal_snapshots(signal_windows: &[SignalWindow]) -> Vec<SignalSnapshot> {
    signal_windows
        .iter()
        .map(|signal| SignalSnapshot {
            name: signal.name.clone(),
            breached_samples: signal.breached_samples,
            window_samples: signal.window_samples,
        })
        .collect()
}

fn build_evidence_input(rationale: String) -> EvidenceInput {
    EvidenceInput {
        rationale,
        prompt_excerpt: Some("runtime-context".to_string()),
        environment: BTreeMap::new(),
        metadata: BTreeMap::new(),
    }
}

fn build_runtime_outcome(
    decision: crate::policy::PolicyDecision,
    evidence_path: Option<PathBuf>,
    fallback_to_non_destructive: bool,
) -> RuntimeOutcome {
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

pub fn run_once(config: &GuardianConfig, input: RuntimeInput) -> RuntimeOutcome {
    let engine = PolicyEngine::new(config.sampling.clone(), config.budgets.clone());
    let decision = engine.decide(build_decision_input(config, &input));

    let mut evidence_path = None;
    let mut fallback_to_non_destructive = false;

    if let Some(proposed_stage) = &decision.proposed_stage {
        let evidence = build_pre_action_evidence(
            input.now,
            input.target_id.clone(),
            proposed_stage.as_str().to_string(),
            decision.rationale.clone(),
            build_signal_snapshots(&input.signal_windows),
            build_evidence_input(decision.rationale.clone()),
        );

        let store = EvidenceStore::new(input.evidence_dir.clone());
        let outcome = persist_pre_action_with_fallback(&store, &evidence);
        evidence_path = outcome.path;
        fallback_to_non_destructive = outcome.fallback_to_non_destructive;
    }

    build_runtime_outcome(decision, evidence_path, fallback_to_non_destructive)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::{Duration, UNIX_EPOCH};

    use tempfile::tempdir;

    use super::{
        RuntimeInput, build_decision_input, build_evidence_input, build_runtime_outcome,
        build_signal_snapshots, run_once,
    };
    use crate::config::{GuardianConfig, Mode};
    use crate::evidence::evidence_exists;
    use crate::policy::{ActionHistoryRecord, PolicyDecision, RemediationStage, SignalWindow};

    fn sample_signal_windows() -> Vec<SignalWindow> {
        vec![
            SignalWindow {
                name: "rss_slope".to_string(),
                breached_samples: 3,
                window_samples: 5,
            },
            SignalWindow {
                name: "orphan_count".to_string(),
                breached_samples: 3,
                window_samples: 5,
            },
        ]
    }

    fn sample_runtime_input(evidence_dir: PathBuf) -> RuntimeInput {
        RuntimeInput {
            target_id: "cli-target".to_string(),
            signal_windows: sample_signal_windows(),
            history: Vec::new(),
            now: UNIX_EPOCH,
            evidence_dir,
        }
    }

    #[test]
    fn build_decision_input_copies_runtime_fields() {
        let mut config = GuardianConfig::default();
        config.mode = Mode::Enforce;
        let input = sample_runtime_input(PathBuf::from("/tmp/evidence"));

        let decision_input = build_decision_input(&config, &input);

        assert_eq!(decision_input.mode, Mode::Enforce);
        assert_eq!(decision_input.target_id, "cli-target");
        assert_eq!(decision_input.signal_windows.len(), 2);
        assert_eq!(decision_input.now, UNIX_EPOCH);
    }

    #[test]
    fn build_signal_snapshots_preserves_signal_metrics() {
        let snapshots = build_signal_snapshots(&sample_signal_windows());

        assert_eq!(snapshots.len(), 2);
        assert_eq!(snapshots[0].name, "rss_slope");
        assert_eq!(snapshots[1].breached_samples, 3);
        assert_eq!(snapshots[1].window_samples, 5);
    }

    #[test]
    fn build_evidence_input_uses_runtime_context_defaults() {
        let input = build_evidence_input("observe_mode_records_without_action".to_string());

        assert_eq!(input.rationale, "observe_mode_records_without_action");
        assert_eq!(input.prompt_excerpt.as_deref(), Some("runtime-context"));
        assert!(input.environment.is_empty());
        assert!(input.metadata.is_empty());
    }

    #[test]
    fn build_runtime_outcome_converts_policy_stage_names() {
        let outcome = build_runtime_outcome(
            PolicyDecision {
                proposed_stage: Some(RemediationStage::WarnThrottle),
                executed_stage: None,
                rationale: "observe_mode_records_without_action".to_string(),
            },
            Some(PathBuf::from("/tmp/evidence.json")),
            true,
        );

        assert_eq!(outcome.proposed_action.as_deref(), Some("warn_throttle"));
        assert_eq!(outcome.executed_action, None);
        assert_eq!(
            outcome.evidence_path,
            Some(PathBuf::from("/tmp/evidence.json"))
        );
        assert!(outcome.fallback_to_non_destructive);
    }

    #[test]
    fn run_once_returns_no_action_when_signal_quorum_is_not_met() {
        let dir = tempdir().expect("tempdir");
        let mut input = sample_runtime_input(dir.path().to_path_buf());
        input.signal_windows[1].breached_samples = 0;

        let outcome = run_once(&GuardianConfig::default(), input);

        assert_eq!(outcome.proposed_action, None);
        assert_eq!(outcome.executed_action, None);
        assert_eq!(outcome.evidence_path, None);
        assert!(!outcome.fallback_to_non_destructive);
    }

    #[test]
    fn run_once_persists_evidence_in_observe_mode() {
        let dir = tempdir().expect("tempdir");
        let config = GuardianConfig::default();

        let outcome = run_once(&config, sample_runtime_input(dir.path().to_path_buf()));

        assert_eq!(outcome.proposed_action.as_deref(), Some("warn_throttle"));
        assert_eq!(outcome.executed_action, None);
        let evidence_path = outcome
            .evidence_path
            .expect("evidence path should be present");
        assert!(evidence_exists(&evidence_path));
        assert!(!outcome.fallback_to_non_destructive);
    }

    #[test]
    fn run_once_executes_action_in_enforce_mode_without_recent_history() {
        let dir = tempdir().expect("tempdir");
        let mut config = GuardianConfig::default();
        config.mode = Mode::Enforce;

        let outcome = run_once(&config, sample_runtime_input(dir.path().to_path_buf()));

        assert_eq!(outcome.proposed_action.as_deref(), Some("warn_throttle"));
        assert_eq!(outcome.executed_action.as_deref(), Some("warn_throttle"));
        assert!(outcome.evidence_path.is_some());
    }

    #[test]
    fn run_once_blocks_execution_when_recent_history_hits_budget() {
        let dir = tempdir().expect("tempdir");
        let mut config = GuardianConfig::default();
        config.mode = Mode::Enforce;
        let mut input = sample_runtime_input(dir.path().to_path_buf());
        input.now = UNIX_EPOCH + Duration::from_secs(3600);
        input.history = vec![ActionHistoryRecord {
            target_id: "cli-target".to_string(),
            stage: RemediationStage::WarnThrottle,
            executed_at: UNIX_EPOCH + Duration::from_secs(3590),
        }];

        let outcome = run_once(&config, input);

        assert_eq!(outcome.proposed_action.as_deref(), Some("warn_throttle"));
        assert_eq!(outcome.executed_action, None);
        assert!(outcome.evidence_path.is_some());
    }
}
