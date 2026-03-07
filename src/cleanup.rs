use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use crate::safety::is_path_allowlisted;

#[derive(Debug, Clone)]
pub struct CleanupPolicy {
    pub allowlist: Vec<PathBuf>,
    pub active_session_grace: Duration,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct CleanupOutcome {
    pub removed: Vec<PathBuf>,
    pub skipped_outside_allowlist: Vec<PathBuf>,
    pub skipped_active_session_grace: Vec<PathBuf>,
}

pub fn remove_stale_allowlisted_artifacts(
    candidates: &[PathBuf],
    policy: &CleanupPolicy,
    now: SystemTime,
) -> std::io::Result<CleanupOutcome> {
    let mut outcome = CleanupOutcome::default();

    for candidate in candidates {
        if !candidate.exists() || !candidate.is_file() {
            continue;
        }

        if !is_path_allowlisted(candidate, &policy.allowlist).unwrap_or(false) {
            outcome.skipped_outside_allowlist.push(candidate.clone());
            continue;
        }

        let metadata = fs::metadata(candidate)?;
        let modified_at = metadata.modified()?;
        let age = now
            .duration_since(modified_at)
            .unwrap_or(Duration::from_secs(0));

        if age < policy.active_session_grace {
            outcome.skipped_active_session_grace.push(candidate.clone());
            continue;
        }

        fs::remove_file(candidate)?;
        outcome.removed.push(candidate.clone());
    }

    Ok(outcome)
}
