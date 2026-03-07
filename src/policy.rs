use std::time::{Duration, SystemTime};

use crate::config::{ActionBudget, Mode, SamplingPolicy};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemediationStage {
    WarnThrottle,
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
            return PolicyDecision {
                proposed_stage: None,
                executed_stage: None,
                rationale: "signal_quorum_not_met".to_string(),
            };
        }

        let proposed_stage = Some(RemediationStage::WarnThrottle);

        if input.mode == Mode::Observe {
            return PolicyDecision {
                proposed_stage,
                executed_stage: None,
                rationale: "observe_mode_records_without_action".to_string(),
            };
        }

        if self.in_cooldown_or_budget_block(&input.target_id, &input.history, input.now) {
            return PolicyDecision {
                proposed_stage,
                executed_stage: None,
                rationale: "cooldown_or_budget_active".to_string(),
            };
        }

        PolicyDecision {
            proposed_stage: Some(RemediationStage::WarnThrottle),
            executed_stage: Some(RemediationStage::WarnThrottle),
            rationale: "step1_warn_throttle".to_string(),
        }
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

        let per_hour = history
            .iter()
            .filter(|entry| entry.target_id == target_id)
            .filter(|entry| {
                now.duration_since(entry.executed_at)
                    .map(|elapsed| elapsed <= one_hour)
                    .unwrap_or(false)
            })
            .count() as u32;

        if per_hour >= self.budgets.max_destructive_per_target_per_hour {
            return true;
        }

        let per_day = history
            .iter()
            .filter(|entry| entry.target_id == target_id)
            .filter(|entry| {
                now.duration_since(entry.executed_at)
                    .map(|elapsed| elapsed <= one_day)
                    .unwrap_or(false)
            })
            .count() as u32;

        per_day >= self.budgets.max_destructive_per_day
    }
}
