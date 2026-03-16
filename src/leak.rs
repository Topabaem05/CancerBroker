use std::collections::{BTreeMap, BTreeSet};

use crate::config::LeakDetectionPolicy;
use crate::monitor::process::{ProcessInventory, ProcessSample};
use crate::safety::{OwnershipPolicy, ProcessIdentity, SafetyDecision, validate_process_identity};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct LeakKey {
    pid: u32,
    start_time_secs: u64,
}

#[derive(Debug, Clone)]
struct LeakHistoryEntry {
    identity: ProcessIdentity,
    baseline_rss_bytes: u64,
    last_rss_bytes: u64,
    sample_count: usize,
    consecutive_growth_samples: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeakCandidate {
    pub identity: ProcessIdentity,
    pub baseline_rss_bytes: u64,
    pub current_rss_bytes: u64,
    pub sample_count: usize,
    pub consecutive_growth_samples: usize,
    pub total_growth_bytes: u64,
}

#[derive(Debug, Clone, Default)]
pub struct LeakDetector {
    histories: BTreeMap<LeakKey, LeakHistoryEntry>,
}

fn build_process_identity(sample: &ProcessSample) -> ProcessIdentity {
    ProcessIdentity {
        pid: sample.pid,
        parent_pid: sample.parent_pid,
        pgid: sample.pgid,
        start_time_secs: sample.start_time_secs,
        uid: sample.uid,
        current_rss_bytes: sample.memory_bytes,
        command: sample.command.clone(),
        listening_ports: sample.listening_ports.clone(),
    }
}

fn build_leak_key(sample: &ProcessSample) -> LeakKey {
    LeakKey {
        pid: sample.pid,
        start_time_secs: sample.start_time_secs,
    }
}

fn is_eligible_sample(
    sample: &ProcessSample,
    leak_policy: &LeakDetectionPolicy,
    ownership_policy: &OwnershipPolicy,
) -> Option<ProcessIdentity> {
    if !leak_policy.enabled || sample.memory_bytes < leak_policy.minimum_rss_bytes {
        return None;
    }

    let identity = build_process_identity(sample);
    match validate_process_identity(&identity, ownership_policy) {
        SafetyDecision::Allowed => Some(identity),
        SafetyDecision::Rejected(_) => None,
    }
}

fn build_leak_candidate(entry: &LeakHistoryEntry) -> LeakCandidate {
    LeakCandidate {
        identity: entry.identity.clone(),
        baseline_rss_bytes: entry.baseline_rss_bytes,
        current_rss_bytes: entry.last_rss_bytes,
        sample_count: entry.sample_count,
        consecutive_growth_samples: entry.consecutive_growth_samples,
        total_growth_bytes: entry
            .last_rss_bytes
            .saturating_sub(entry.baseline_rss_bytes),
    }
}

impl LeakDetector {
    pub fn observe_inventory(
        &mut self,
        inventory: &ProcessInventory,
        leak_policy: &LeakDetectionPolicy,
        ownership_policy: &OwnershipPolicy,
    ) -> Vec<LeakCandidate> {
        if !leak_policy.enabled {
            self.histories.clear();
            return Vec::new();
        }

        let mut seen = BTreeSet::new();
        let mut candidates = Vec::new();

        for sample in inventory.samples() {
            let Some(identity) = is_eligible_sample(sample, leak_policy, ownership_policy) else {
                continue;
            };

            let key = build_leak_key(sample);
            seen.insert(key.clone());

            let entry = self
                .histories
                .entry(key)
                .or_insert_with(|| LeakHistoryEntry {
                    identity: identity.clone(),
                    baseline_rss_bytes: sample.memory_bytes,
                    last_rss_bytes: sample.memory_bytes,
                    sample_count: 1,
                    consecutive_growth_samples: 0,
                });

            entry.identity = identity;

            if sample.memory_bytes
                >= entry.last_rss_bytes + leak_policy.minimum_growth_bytes_per_sample
            {
                entry.last_rss_bytes = sample.memory_bytes;
                entry.sample_count += 1;
                entry.consecutive_growth_samples += 1;
            } else if sample.memory_bytes != entry.last_rss_bytes {
                entry.baseline_rss_bytes = sample.memory_bytes;
                entry.last_rss_bytes = sample.memory_bytes;
                entry.sample_count = 1;
                entry.consecutive_growth_samples = 0;
            }

            if entry.consecutive_growth_samples >= leak_policy.required_consecutive_growth_samples {
                candidates.push(build_leak_candidate(entry));
            }
        }

        self.histories.retain(|key, _| seen.contains(key));
        candidates
    }
}

#[cfg(test)]
mod tests {
    use super::LeakDetector;
    use crate::config::LeakDetectionPolicy;
    use crate::monitor::process::{ProcessInventory, ProcessSample};
    use crate::safety::OwnershipPolicy;

    fn leak_policy() -> LeakDetectionPolicy {
        LeakDetectionPolicy {
            enabled: true,
            required_consecutive_growth_samples: 2,
            minimum_rss_bytes: 100,
            minimum_growth_bytes_per_sample: 20,
        }
    }

    fn ownership_policy() -> OwnershipPolicy {
        OwnershipPolicy {
            expected_uid: 501,
            required_command_markers: vec!["opencode".to_string()],
            same_uid_only: true,
        }
    }

    fn sample_process(start_time_secs: u64, memory_bytes: u64, command: &str) -> ProcessSample {
        ProcessSample {
            pid: 10,
            parent_pid: Some(1),
            pgid: Some(10),
            start_time_secs,
            uid: Some(501),
            memory_bytes,
            cpu_percent: 0.5,
            command: command.to_string(),
            listening_ports: vec![],
        }
    }

    #[test]
    fn detector_emits_candidate_after_required_growth_samples() {
        let mut detector = LeakDetector::default();
        let policy = leak_policy();
        let ownership = ownership_policy();

        let first = ProcessInventory::from_samples([sample_process(42, 100, "opencode ses_alpha")]);
        let second =
            ProcessInventory::from_samples([sample_process(42, 140, "opencode ses_alpha")]);
        let third = ProcessInventory::from_samples([sample_process(42, 170, "opencode ses_alpha")]);

        assert!(
            detector
                .observe_inventory(&first, &policy, &ownership)
                .is_empty()
        );
        assert!(
            detector
                .observe_inventory(&second, &policy, &ownership)
                .is_empty()
        );

        let candidates = detector.observe_inventory(&third, &policy, &ownership);

        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].identity.pid, 10);
        assert_eq!(candidates[0].baseline_rss_bytes, 100);
        assert_eq!(candidates[0].current_rss_bytes, 170);
        assert_eq!(candidates[0].sample_count, 3);
        assert_eq!(candidates[0].consecutive_growth_samples, 2);
        assert_eq!(candidates[0].total_growth_bytes, 70);
    }

    #[test]
    fn detector_resets_growth_when_rss_drops() {
        let mut detector = LeakDetector::default();
        let policy = leak_policy();
        let ownership = ownership_policy();

        let inventories = [
            ProcessInventory::from_samples([sample_process(42, 100, "opencode ses_alpha")]),
            ProcessInventory::from_samples([sample_process(42, 140, "opencode ses_alpha")]),
            ProcessInventory::from_samples([sample_process(42, 120, "opencode ses_alpha")]),
            ProcessInventory::from_samples([sample_process(42, 145, "opencode ses_alpha")]),
        ];

        for inventory in &inventories {
            assert!(
                detector
                    .observe_inventory(inventory, &policy, &ownership)
                    .is_empty()
            );
        }
    }

    #[test]
    fn detector_ignores_processes_below_minimum_rss() {
        let mut detector = LeakDetector::default();
        let policy = leak_policy();
        let ownership = ownership_policy();

        let inventory =
            ProcessInventory::from_samples([sample_process(42, 90, "opencode ses_alpha")]);

        assert!(
            detector
                .observe_inventory(&inventory, &policy, &ownership)
                .is_empty()
        );
    }

    #[test]
    fn detector_resets_when_process_restarts() {
        let mut detector = LeakDetector::default();
        let policy = leak_policy();
        let ownership = ownership_policy();

        let first = ProcessInventory::from_samples([sample_process(42, 100, "opencode ses_alpha")]);
        let second =
            ProcessInventory::from_samples([sample_process(42, 140, "opencode ses_alpha")]);
        let restarted =
            ProcessInventory::from_samples([sample_process(99, 150, "opencode ses_alpha")]);
        let final_sample =
            ProcessInventory::from_samples([sample_process(99, 180, "opencode ses_alpha")]);

        assert!(
            detector
                .observe_inventory(&first, &policy, &ownership)
                .is_empty()
        );
        assert!(
            detector
                .observe_inventory(&second, &policy, &ownership)
                .is_empty()
        );
        assert!(
            detector
                .observe_inventory(&restarted, &policy, &ownership)
                .is_empty()
        );
        assert!(
            detector
                .observe_inventory(&final_sample, &policy, &ownership)
                .is_empty()
        );
    }

    #[test]
    fn detector_ignores_processes_that_fail_ownership_policy() {
        let mut detector = LeakDetector::default();
        let policy = leak_policy();
        let ownership = ownership_policy();
        let inventory = ProcessInventory::from_samples([sample_process(42, 150, "python worker")]);

        assert!(
            detector
                .observe_inventory(&inventory, &policy, &ownership)
                .is_empty()
        );
    }
}
