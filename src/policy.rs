use std::time::{Duration, SystemTime};

use crate::config::{ActionBudget, Mode, SamplingPolicy};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemediationStage {
    WarnThrottle,
}

impl RemediationStage {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::WarnThrottle => "warn_throttle",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignalWindow {
    pub name: String,
    pub breached_samples: usize,
    pub window_samples: usize,
}

#[derive(Debug, Clone)]
pub struct ActionHistoryRecord {
    pub target_id: String,
    pub stage: RemediationStage,
    pub executed_at: SystemTime,
}

#[derive(Debug, Clone)]
pub struct DecisionInput {
    pub mode: Mode,
    pub target_id: String,
    pub signal_windows: Vec<SignalWindow>,
    pub history: Vec<ActionHistoryRecord>,
    pub now: SystemTime,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyDecision {
    pub proposed_stage: Option<RemediationStage>,
    pub executed_stage: Option<RemediationStage>,
    pub rationale: String,
}

#[derive(Debug, Clone)]
pub struct PolicyEngine {
    sampling: SamplingPolicy,
    budgets: ActionBudget,
}

fn build_policy_decision(
    proposed_stage: Option<RemediationStage>,
    executed_stage: Option<RemediationStage>,
    rationale: &str,
) -> PolicyDecision {
    PolicyDecision {
        proposed_stage,
        executed_stage,
        rationale: rationale.to_string(),
    }
}

fn count_recent_actions(
    target_id: &str,
    history: &[ActionHistoryRecord],
    now: SystemTime,
    window: Duration,
) -> u32 {
    history
        .iter()
        .filter(|entry| entry.target_id == target_id)
        .filter(|entry| {
            now.duration_since(entry.executed_at)
                .map(|elapsed| elapsed <= window)
                .unwrap_or(false)
        })
        .count() as u32
}

impl PolicyEngine {
    pub fn new(sampling: SamplingPolicy, budgets: ActionBudget) -> Self {
        Self { sampling, budgets }
    }

    pub fn decide(&self, input: DecisionInput) -> PolicyDecision {
        let eligible_signals = input
            .signal_windows
            .iter()
            .filter(|signal| self.signal_is_eligible(signal))
            .count();

        if eligible_signals < self.sampling.signal_quorum {
            return build_policy_decision(None, None, "signal_quorum_not_met");
        }

        let proposed_stage = Some(RemediationStage::WarnThrottle);

        if input.mode == Mode::Observe {
            return build_policy_decision(
                proposed_stage,
                None,
                "observe_mode_records_without_action",
            );
        }

        if self.in_cooldown_or_budget_block(&input.target_id, &input.history, input.now) {
            return build_policy_decision(proposed_stage, None, "cooldown_or_budget_active");
        }

        build_policy_decision(
            Some(RemediationStage::WarnThrottle),
            Some(RemediationStage::WarnThrottle),
            "step1_warn_throttle",
        )
    }

    fn signal_is_eligible(&self, signal: &SignalWindow) -> bool {
        signal.window_samples >= self.sampling.breach_window_samples
            && signal.breached_samples >= self.sampling.breach_required_samples
    }

    fn in_cooldown_or_budget_block(
        &self,
        target_id: &str,
        history: &[ActionHistoryRecord],
        now: SystemTime,
    ) -> bool {
        let one_hour = Duration::from_secs(60 * 60);
        let one_day = Duration::from_secs(60 * 60 * 24);

        let per_hour = count_recent_actions(target_id, history, now, one_hour);

        if per_hour >= self.budgets.max_destructive_per_target_per_hour {
            return true;
        }

        let per_day = count_recent_actions(target_id, history, now, one_day);

        per_day >= self.budgets.max_destructive_per_day
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, UNIX_EPOCH};

    use super::{
        ActionHistoryRecord, DecisionInput, PolicyEngine, RemediationStage, SignalWindow,
        build_policy_decision, count_recent_actions,
    };
    use crate::config::{ActionBudget, Mode, SamplingPolicy};

    fn sample_engine() -> PolicyEngine {
        PolicyEngine::new(SamplingPolicy::default(), ActionBudget::default())
    }

    fn eligible_signals() -> Vec<SignalWindow> {
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

    fn decision_input(mode: Mode) -> DecisionInput {
        DecisionInput {
            mode,
            target_id: "cli-target".to_string(),
            signal_windows: eligible_signals(),
            history: Vec::new(),
            now: UNIX_EPOCH + Duration::from_secs(3600),
        }
    }

    #[test]
    fn build_policy_decision_preserves_stage_and_rationale() {
        let decision = build_policy_decision(
            Some(RemediationStage::WarnThrottle),
            None,
            "observe_mode_records_without_action",
        );

        assert_eq!(
            decision.proposed_stage,
            Some(RemediationStage::WarnThrottle)
        );
        assert_eq!(decision.executed_stage, None);
        assert_eq!(decision.rationale, "observe_mode_records_without_action");
    }

    #[test]
    fn count_recent_actions_counts_only_matching_target_within_window() {
        let history = vec![
            ActionHistoryRecord {
                target_id: "cli-target".to_string(),
                stage: RemediationStage::WarnThrottle,
                executed_at: UNIX_EPOCH + Duration::from_secs(3500),
            },
            ActionHistoryRecord {
                target_id: "other-target".to_string(),
                stage: RemediationStage::WarnThrottle,
                executed_at: UNIX_EPOCH + Duration::from_secs(3500),
            },
            ActionHistoryRecord {
                target_id: "cli-target".to_string(),
                stage: RemediationStage::WarnThrottle,
                executed_at: UNIX_EPOCH,
            },
        ];

        let count = count_recent_actions(
            "cli-target",
            &history,
            UNIX_EPOCH + Duration::from_secs(3601),
            Duration::from_secs(3600),
        );

        assert_eq!(count, 1);
    }

    #[test]
    fn decide_returns_no_action_when_signal_quorum_is_not_met() {
        let mut input = decision_input(Mode::Observe);
        input.signal_windows[1].breached_samples = 0;

        let decision = sample_engine().decide(input);

        assert_eq!(decision.proposed_stage, None);
        assert_eq!(decision.executed_stage, None);
        assert_eq!(decision.rationale, "signal_quorum_not_met");
    }

    #[test]
    fn decide_records_without_execution_in_observe_mode() {
        let decision = sample_engine().decide(decision_input(Mode::Observe));

        assert_eq!(
            decision.proposed_stage,
            Some(RemediationStage::WarnThrottle)
        );
        assert_eq!(decision.executed_stage, None);
        assert_eq!(decision.rationale, "observe_mode_records_without_action");
    }

    #[test]
    fn decide_blocks_execution_when_hourly_budget_is_reached() {
        let mut input = decision_input(Mode::Enforce);
        input.history = vec![ActionHistoryRecord {
            target_id: "cli-target".to_string(),
            stage: RemediationStage::WarnThrottle,
            executed_at: UNIX_EPOCH + Duration::from_secs(3590),
        }];

        let decision = sample_engine().decide(input);

        assert_eq!(
            decision.proposed_stage,
            Some(RemediationStage::WarnThrottle)
        );
        assert_eq!(decision.executed_stage, None);
        assert_eq!(decision.rationale, "cooldown_or_budget_active");
    }

    #[test]
    fn decide_executes_warn_throttle_in_enforce_mode_without_budget_block() {
        let decision = sample_engine().decide(decision_input(Mode::Enforce));

        assert_eq!(
            decision.proposed_stage,
            Some(RemediationStage::WarnThrottle)
        );
        assert_eq!(
            decision.executed_stage,
            Some(RemediationStage::WarnThrottle)
        );
        assert_eq!(decision.rationale, "step1_warn_throttle");
    }
}
