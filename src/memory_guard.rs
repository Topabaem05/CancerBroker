use std::collections::{BTreeMap, BTreeSet};
use std::time::SystemTime;

use crate::config::RustAnalyzerMemoryGuardPolicy;
use crate::monitor::process::{ProcessInventory, ProcessSample};
use crate::safety::{OwnershipPolicy, ProcessIdentity, SafetyDecision, validate_process_identity};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct GuardKey {
    pid: u32,
    start_time_secs: u64,
}

#[derive(Debug, Clone)]
struct GuardHistoryEntry {
    identity: ProcessIdentity,
    current_rss_bytes: u64,
    consecutive_over_limit_samples: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryGuardCandidate {
    pub identity: ProcessIdentity,
    pub current_rss_bytes: u64,
    pub consecutive_over_limit_samples: usize,
}

#[derive(Debug, Clone, Default)]
pub struct RustAnalyzerMemoryGuard {
    histories: BTreeMap<GuardKey, GuardHistoryEntry>,
    last_remediation_unix_secs: Option<u64>,
}

fn unix_timestamp_secs(now: SystemTime) -> u64 {
    now.duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn build_guard_key(sample: &ProcessSample) -> GuardKey {
    GuardKey {
        pid: sample.pid,
        start_time_secs: sample.start_time_secs,
    }
}

fn build_process_identity(sample: &ProcessSample) -> ProcessIdentity {
    ProcessIdentity {
        pid: sample.pid,
        parent_pid: sample.parent_pid,
        pgid: sample.pgid,
        start_time_secs: sample.start_time_secs,
        uid: sample.uid,
        command: sample.command.clone(),
        listening_ports: sample.listening_ports.clone(),
    }
}

fn command_contains_rust_analyzer(command: &str) -> bool {
    command.to_ascii_lowercase().contains("rust-analyzer")
}

fn build_candidate(entry: &GuardHistoryEntry) -> MemoryGuardCandidate {
    MemoryGuardCandidate {
        identity: entry.identity.clone(),
        current_rss_bytes: entry.current_rss_bytes,
        consecutive_over_limit_samples: entry.consecutive_over_limit_samples,
    }
}

fn sample_is_past_startup_grace(
    sample: &ProcessSample,
    policy: &RustAnalyzerMemoryGuardPolicy,
    now_unix_secs: u64,
) -> bool {
    now_unix_secs.saturating_sub(sample.start_time_secs) >= policy.startup_grace_secs
}

fn sample_matches_guard(
    sample: &ProcessSample,
    policy: &RustAnalyzerMemoryGuardPolicy,
    ownership_policy: &OwnershipPolicy,
    now_unix_secs: u64,
) -> Option<ProcessIdentity> {
    if !policy.enabled || !command_contains_rust_analyzer(&sample.command) {
        return None;
    }

    if !sample_is_past_startup_grace(sample, policy, now_unix_secs) {
        return None;
    }

    let identity = build_process_identity(sample);
    match validate_process_identity(&identity, ownership_policy) {
        SafetyDecision::Allowed => Some(identity),
        SafetyDecision::Rejected(_) => None,
    }
}

impl RustAnalyzerMemoryGuard {
    pub fn observe_inventory(
        &mut self,
        inventory: &ProcessInventory,
        policy: &RustAnalyzerMemoryGuardPolicy,
        ownership_policy: &OwnershipPolicy,
        now: SystemTime,
    ) -> Vec<MemoryGuardCandidate> {
        if !policy.enabled {
            self.histories.clear();
            self.last_remediation_unix_secs = None;
            return Vec::new();
        }

        let now_unix_secs = unix_timestamp_secs(now);
        let cooldown_active = self
            .last_remediation_unix_secs
            .is_some_and(|last| now_unix_secs.saturating_sub(last) < policy.cooldown_secs);

        let mut seen = BTreeSet::new();
        let mut candidates = Vec::new();

        for sample in inventory.samples() {
            let Some(identity) =
                sample_matches_guard(sample, policy, ownership_policy, now_unix_secs)
            else {
                continue;
            };

            let key = build_guard_key(sample);
            seen.insert(key.clone());

            let entry = self
                .histories
                .entry(key)
                .or_insert_with(|| GuardHistoryEntry {
                    identity: identity.clone(),
                    current_rss_bytes: sample.memory_bytes,
                    consecutive_over_limit_samples: 0,
                });

            entry.identity = identity;
            entry.current_rss_bytes = sample.memory_bytes;

            if sample.memory_bytes >= policy.max_rss_bytes {
                entry.consecutive_over_limit_samples += 1;
            } else {
                entry.consecutive_over_limit_samples = 0;
            }

            if !cooldown_active
                && entry.consecutive_over_limit_samples >= policy.required_consecutive_samples
            {
                candidates.push(build_candidate(entry));
            }
        }

        self.histories.retain(|key, _| seen.contains(key));
        candidates
    }

    pub fn record_remediation(&mut self, now: SystemTime) {
        self.last_remediation_unix_secs = Some(unix_timestamp_secs(now));
        self.histories.clear();
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, UNIX_EPOCH};

    use super::RustAnalyzerMemoryGuard;
    use crate::config::RustAnalyzerMemoryGuardPolicy;
    use crate::monitor::process::{ProcessInventory, ProcessSample};
    use crate::safety::OwnershipPolicy;

    fn policy() -> RustAnalyzerMemoryGuardPolicy {
        RustAnalyzerMemoryGuardPolicy {
            enabled: true,
            max_rss_bytes: 500,
            required_consecutive_samples: 2,
            startup_grace_secs: 60,
            cooldown_secs: 300,
            same_uid_only: true,
        }
    }

    fn ownership_policy() -> OwnershipPolicy {
        OwnershipPolicy {
            expected_uid: 501,
            required_command_markers: vec!["rust-analyzer".to_string()],
            same_uid_only: true,
        }
    }

    fn sample_process(start_time_secs: u64, memory_bytes: u64, command: &str) -> ProcessSample {
        ProcessSample {
            pid: 10,
            parent_pid: Some(1),
            pgid: None,
            start_time_secs,
            uid: Some(501),
            memory_bytes,
            cpu_percent: 0.2,
            command: command.to_string(),
            listening_ports: vec![],
        }
    }

    #[test]
    fn memory_guard_emits_candidate_after_required_samples() {
        let mut guard = RustAnalyzerMemoryGuard::default();
        let policy = policy();
        let ownership = ownership_policy();
        let now = UNIX_EPOCH + Duration::from_secs(400);

        let first = ProcessInventory::from_samples([sample_process(100, 550, "rust-analyzer")]);
        let second = ProcessInventory::from_samples([sample_process(100, 560, "rust-analyzer")]);

        assert!(
            guard
                .observe_inventory(&first, &policy, &ownership, now)
                .is_empty()
        );

        let candidates = guard.observe_inventory(&second, &policy, &ownership, now);

        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].current_rss_bytes, 560);
        assert_eq!(candidates[0].consecutive_over_limit_samples, 2);
    }

    #[test]
    fn memory_guard_respects_startup_grace_and_cooldown() {
        let mut guard = RustAnalyzerMemoryGuard::default();
        let policy = policy();
        let ownership = ownership_policy();
        let early_now = UNIX_EPOCH + Duration::from_secs(120);
        let steady_now = UNIX_EPOCH + Duration::from_secs(400);
        let inventory = ProcessInventory::from_samples([sample_process(100, 600, "rust-analyzer")]);

        assert!(
            guard
                .observe_inventory(&inventory, &policy, &ownership, early_now)
                .is_empty()
        );
        assert!(
            guard
                .observe_inventory(&inventory, &policy, &ownership, steady_now)
                .is_empty()
        );
        assert_eq!(
            guard
                .observe_inventory(&inventory, &policy, &ownership, steady_now)
                .len(),
            1
        );

        guard.record_remediation(steady_now);
        assert!(
            guard
                .observe_inventory(&inventory, &policy, &ownership, steady_now)
                .is_empty()
        );
    }

    #[test]
    fn memory_guard_ignores_non_matching_processes() {
        let mut guard = RustAnalyzerMemoryGuard::default();
        let policy = policy();
        let ownership = ownership_policy();
        let now = UNIX_EPOCH + Duration::from_secs(400);
        let inventory = ProcessInventory::from_samples([sample_process(100, 600, "cargo check")]);

        assert!(
            guard
                .observe_inventory(&inventory, &policy, &ownership, now)
                .is_empty()
        );
    }
}
