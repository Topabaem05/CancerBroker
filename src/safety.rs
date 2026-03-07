use std::fs;
use std::path::{Path, PathBuf};

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessIdentity {
    pub pid: u32,
    pub parent_pid: Option<u32>,
    pub start_time_secs: u64,
    pub uid: Option<u32>,
    pub command: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnershipPolicy {
    pub expected_uid: u32,
    pub required_command_markers: Vec<String>,
    pub same_uid_only: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SafetyDecision {
    Allowed,
    Rejected(&'static str),
}

#[derive(Debug, Error)]
pub enum SafetyError {
    #[error("path canonicalization failed for {path}: {source}")]
    Canonicalize {
        path: String,
        source: std::io::Error,
    },
}

pub fn validate_process_identity(
    identity: &ProcessIdentity,
    policy: &OwnershipPolicy,
) -> SafetyDecision {
    if policy.same_uid_only && identity.uid != Some(policy.expected_uid) {
        return SafetyDecision::Rejected("uid_mismatch");
    }

    if !policy.required_command_markers.is_empty() {
        let command = identity.command.to_lowercase();
        let marker_match = policy
            .required_command_markers
            .iter()
            .any(|marker| command.contains(&marker.to_lowercase()));

        if !marker_match {
            return SafetyDecision::Rejected("command_marker_mismatch");
        }
    }

    SafetyDecision::Allowed
}

pub fn canonicalize_policy_path(path: &Path) -> Result<PathBuf, SafetyError> {
    fs::canonicalize(path).map_err(|source| SafetyError::Canonicalize {
        path: path.display().to_string(),
        source,
    })
}

pub fn is_path_allowlisted(candidate: &Path, allowlist: &[PathBuf]) -> Result<bool, SafetyError> {
    if allowlist.is_empty() {
        return Ok(false);
    }

    let candidate_canonical = canonicalize_policy_path(candidate)?;

    for allow_root in allowlist {
        let root_canonical = canonicalize_policy_path(allow_root)?;
        if candidate_canonical.starts_with(&root_canonical) {
            return Ok(true);
        }
    }

    Ok(false)
}
