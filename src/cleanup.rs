use std::fs;
use std::path::{Path, PathBuf};
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

fn candidate_age(candidate: &Path, now: SystemTime) -> std::io::Result<Duration> {
    let metadata = fs::metadata(candidate)?;
    let modified_at = metadata.modified()?;

    Ok(now
        .duration_since(modified_at)
        .unwrap_or(Duration::from_secs(0)))
}

fn is_within_active_session_grace(age: Duration, policy: &CleanupPolicy) -> bool {
    age < policy.active_session_grace
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

        let age = candidate_age(candidate, now)?;

        if is_within_active_session_grace(age, policy) {
            outcome.skipped_active_session_grace.push(candidate.clone());
            continue;
        }

        fs::remove_file(candidate)?;
        outcome.removed.push(candidate.clone());
    }

    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{Duration, SystemTime};

    use tempfile::tempdir;

    use super::{
        CleanupPolicy, candidate_age, is_within_active_session_grace,
        remove_stale_allowlisted_artifacts,
    };

    #[test]
    fn candidate_age_clamps_future_modified_times() {
        let dir = tempdir().expect("tempdir");
        let file = dir.path().join("artifact.json");
        fs::write(&file, "{}").expect("artifact should be written");
        let modified_at = fs::metadata(&file)
            .expect("metadata should exist")
            .modified()
            .expect("mtime should exist");

        let age = candidate_age(
            &file,
            modified_at
                .checked_sub(Duration::from_secs(1))
                .expect("earlier timestamp should be representable"),
        )
        .expect("age should compute");

        assert_eq!(age, Duration::from_secs(0));
    }

    #[test]
    fn remove_stale_allowlisted_artifacts_removes_old_allowlisted_files() {
        let dir = tempdir().expect("tempdir");
        let file = dir.path().join("artifact.json");
        fs::write(&file, "{}").expect("artifact should be written");
        let modified_at = fs::metadata(&file)
            .expect("metadata should exist")
            .modified()
            .expect("mtime should exist");
        let policy = CleanupPolicy {
            allowlist: vec![dir.path().to_path_buf()],
            active_session_grace: Duration::from_secs(1),
        };

        let outcome = remove_stale_allowlisted_artifacts(
            std::slice::from_ref(&file),
            &policy,
            modified_at + Duration::from_secs(10),
        )
        .expect("cleanup should work");

        assert_eq!(outcome.removed, vec![file.clone()]);
        assert!(!file.exists());
    }

    #[test]
    fn remove_stale_allowlisted_artifacts_skips_recent_files_in_grace_window() {
        let dir = tempdir().expect("tempdir");
        let file = dir.path().join("artifact.json");
        fs::write(&file, "{}").expect("artifact should be written");
        let modified_at = fs::metadata(&file)
            .expect("metadata should exist")
            .modified()
            .expect("mtime should exist");
        let policy = CleanupPolicy {
            allowlist: vec![dir.path().to_path_buf()],
            active_session_grace: Duration::from_secs(60),
        };

        let outcome = remove_stale_allowlisted_artifacts(
            std::slice::from_ref(&file),
            &policy,
            modified_at + Duration::from_secs(1),
        )
        .expect("cleanup should work");

        assert_eq!(outcome.skipped_active_session_grace, vec![file.clone()]);
        assert!(file.exists());
    }

    #[test]
    fn remove_stale_allowlisted_artifacts_skips_files_outside_allowlist() {
        let dir = tempdir().expect("tempdir");
        let allow_dir = dir.path().join("allow");
        let outside_dir = dir.path().join("outside");
        fs::create_dir_all(&allow_dir).expect("allow dir should exist");
        fs::create_dir_all(&outside_dir).expect("outside dir should exist");
        let file = outside_dir.join("artifact.json");
        fs::write(&file, "{}").expect("artifact should be written");
        let modified_at = fs::metadata(&file)
            .expect("metadata should exist")
            .modified()
            .expect("mtime should exist");
        let policy = CleanupPolicy {
            allowlist: vec![allow_dir],
            active_session_grace: Duration::from_secs(1),
        };

        let outcome = remove_stale_allowlisted_artifacts(
            std::slice::from_ref(&file),
            &policy,
            modified_at + Duration::from_secs(10),
        )
        .expect("cleanup should work");

        assert_eq!(outcome.skipped_outside_allowlist, vec![file.clone()]);
        assert!(file.exists());
    }

    #[test]
    fn remove_stale_allowlisted_artifacts_ignores_missing_paths_and_directories() {
        let dir = tempdir().expect("tempdir");
        let missing = dir.path().join("missing.json");
        let nested_dir = dir.path().join("nested");
        fs::create_dir_all(&nested_dir).expect("nested dir should exist");
        let policy = CleanupPolicy {
            allowlist: vec![dir.path().to_path_buf()],
            active_session_grace: Duration::from_secs(1),
        };

        let outcome =
            remove_stale_allowlisted_artifacts(&[missing, nested_dir], &policy, SystemTime::now())
                .expect("cleanup should work");

        assert!(outcome.removed.is_empty());
        assert!(outcome.skipped_outside_allowlist.is_empty());
        assert!(outcome.skipped_active_session_grace.is_empty());
    }

    #[test]
    fn active_session_grace_check_is_strictly_less_than_policy_window() {
        let policy = CleanupPolicy {
            allowlist: vec![],
            active_session_grace: Duration::from_secs(5),
        };

        assert!(is_within_active_session_grace(
            Duration::from_secs(4),
            &policy
        ));
        assert!(!is_within_active_session_grace(
            Duration::from_secs(5),
            &policy
        ));
    }
}
