use std::fs;
use std::path::{Path, PathBuf};

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessIdentity {
    pub pid: u32,
    pub parent_pid: Option<u32>,
    pub pgid: Option<u32>,
    pub start_time_secs: u64,
    pub uid: Option<u32>,
    pub current_rss_bytes: u64,
    pub allow_inherited_command_match: bool,
    pub command: String,
    pub listening_ports: Vec<u16>,
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

fn command_matches_policy(command: &str, required_command_markers: &[String]) -> bool {
    if required_command_markers.is_empty() {
        return true;
    }

    let command = command.to_lowercase();
    required_command_markers
        .iter()
        .any(|marker| command.contains(&marker.to_lowercase()))
}

fn command_token(command: &str) -> &str {
    command.split_whitespace().next().unwrap_or(command)
}

fn token_basename(token: &str) -> &str {
    token.rsplit(['/', '\\']).next().unwrap_or(token)
}

fn is_node_child_process(command: &str) -> bool {
    token_basename(command_token(command)).eq_ignore_ascii_case("node")
}

fn command_matches_policy_exact_basename(
    command: &str,
    required_command_markers: &[String],
) -> bool {
    if required_command_markers.is_empty() {
        return true;
    }

    let basename = token_basename(command_token(command));
    required_command_markers
        .iter()
        .any(|marker| basename.eq_ignore_ascii_case(marker))
}

pub fn validate_process_identity(
    identity: &ProcessIdentity,
    policy: &OwnershipPolicy,
) -> SafetyDecision {
    if policy.same_uid_only && identity.uid != Some(policy.expected_uid) {
        return SafetyDecision::Rejected("uid_mismatch");
    }

    if !(command_matches_policy(&identity.command, &policy.required_command_markers)
        || identity.allow_inherited_command_match && is_node_child_process(&identity.command))
    {
        return SafetyDecision::Rejected("command_marker_mismatch");
    }

    SafetyDecision::Allowed
}

pub fn validate_process_identity_or_opencode_parent_node(
    identity: &ProcessIdentity,
    parent_identity: Option<&ProcessIdentity>,
    session_associated: bool,
    policy: &OwnershipPolicy,
) -> SafetyDecision {
    if policy.same_uid_only && identity.uid != Some(policy.expected_uid) {
        return SafetyDecision::Rejected("uid_mismatch");
    }

    if command_matches_policy(&identity.command, &policy.required_command_markers) {
        return SafetyDecision::Allowed;
    }

    if is_node_child_process(&identity.command)
        && ((parent_identity.is_some_and(|parent_identity| {
            (!policy.same_uid_only || parent_identity.uid == Some(policy.expected_uid))
                && command_matches_policy_exact_basename(
                    &parent_identity.command,
                    &policy.required_command_markers,
                )
        })) || session_associated)
    {
        return SafetyDecision::Allowed;
    }

    SafetyDecision::Rejected("command_marker_mismatch")
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
    allowlist_contains_path(&candidate_canonical, allowlist)
}

fn allowlist_contains_path(
    candidate_canonical: &Path,
    allowlist: &[PathBuf],
) -> Result<bool, SafetyError> {
    for allow_root in allowlist {
        let root_canonical = canonicalize_policy_path(allow_root)?;
        if candidate_canonical.starts_with(&root_canonical) {
            return Ok(true);
        }
    }

    Ok(false)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::{
        OwnershipPolicy, ProcessIdentity, SafetyDecision, allowlist_contains_path,
        canonicalize_policy_path, command_matches_policy, command_matches_policy_exact_basename,
        is_node_child_process, is_path_allowlisted, validate_process_identity,
        validate_process_identity_or_opencode_parent_node,
    };

    fn sample_identity() -> ProcessIdentity {
        ProcessIdentity {
            pid: 1,
            parent_pid: None,
            pgid: Some(1),
            start_time_secs: 0,
            uid: Some(1000),
            current_rss_bytes: 0,
            allow_inherited_command_match: false,
            command: "opencode worker".to_string(),
            listening_ports: vec![],
        }
    }

    fn sample_policy() -> OwnershipPolicy {
        OwnershipPolicy {
            expected_uid: 1000,
            required_command_markers: vec!["opencode".to_string(), "openagent".to_string()],
            same_uid_only: true,
        }
    }

    #[test]
    fn command_matches_policy_handles_case_insensitive_markers() {
        assert!(command_matches_policy(
            "OpenCode Worker",
            &["opencode".to_string()]
        ));
        assert!(!command_matches_policy(
            "bash worker",
            &["opencode".to_string()]
        ));
    }

    #[test]
    fn validate_process_identity_allows_matching_uid_and_marker() {
        let decision = validate_process_identity(&sample_identity(), &sample_policy());

        assert_eq!(decision, SafetyDecision::Allowed);
    }

    #[test]
    fn validate_process_identity_rejects_uid_mismatch() {
        let mut identity = sample_identity();
        identity.uid = Some(2000);

        let decision = validate_process_identity(&identity, &sample_policy());

        assert_eq!(decision, SafetyDecision::Rejected("uid_mismatch"));
    }

    #[test]
    fn validate_process_identity_rejects_command_marker_mismatch() {
        let mut identity = sample_identity();
        identity.command = "python worker".to_string();

        let decision = validate_process_identity(&identity, &sample_policy());

        assert_eq!(
            decision,
            SafetyDecision::Rejected("command_marker_mismatch")
        );
    }

    #[test]
    fn exact_basename_match_avoids_path_only_false_positives() {
        assert!(command_matches_policy_exact_basename(
            "/usr/local/bin/opencode ses_alpha",
            &["opencode".to_string()]
        ));
        assert!(!command_matches_policy_exact_basename(
            "bun run /Users/name/.config/opencode/server.ts",
            &["opencode".to_string()]
        ));
    }

    #[test]
    fn validate_process_identity_or_opencode_parent_node_allows_node_child_of_opencode() {
        let mut identity = sample_identity();
        identity.command = "node worker.js".to_string();

        let mut parent = sample_identity();
        parent.command = "opencode ses_alpha worker".to_string();

        let decision = validate_process_identity_or_opencode_parent_node(
            &identity,
            Some(&parent),
            false,
            &sample_policy(),
        );

        assert_eq!(decision, SafetyDecision::Allowed);
    }

    #[test]
    fn validate_process_identity_or_opencode_parent_node_rejects_unrelated_node() {
        let mut identity = sample_identity();
        identity.command = "node worker.js".to_string();

        let mut parent = sample_identity();
        parent.command = "bash worker".to_string();

        let decision = validate_process_identity_or_opencode_parent_node(
            &identity,
            Some(&parent),
            false,
            &sample_policy(),
        );

        assert_eq!(
            decision,
            SafetyDecision::Rejected("command_marker_mismatch")
        );
    }

    #[test]
    fn validate_process_identity_or_opencode_parent_node_allows_session_associated_node() {
        let mut identity = sample_identity();
        identity.command = "node worker.js".to_string();

        let decision = validate_process_identity_or_opencode_parent_node(
            &identity,
            None,
            true,
            &sample_policy(),
        );

        assert_eq!(decision, SafetyDecision::Allowed);
    }

    #[test]
    fn node_child_detection_uses_executable_basename() {
        assert!(is_node_child_process("node worker.js"));
        assert!(is_node_child_process("/usr/local/bin/node worker.js"));
        assert!(!is_node_child_process("bun worker.js"));
    }

    #[test]
    fn canonicalize_policy_path_returns_absolute_path() {
        let dir = tempdir().expect("tempdir");
        let path = canonicalize_policy_path(dir.path()).expect("path should canonicalize");

        assert!(path.is_absolute());
    }

    #[test]
    fn allowlist_contains_path_matches_nested_children() {
        let dir = tempdir().expect("tempdir");
        let allow_root = dir.path().join("allow");
        let child_dir = allow_root.join("nested");
        fs::create_dir_all(&child_dir).expect("child dir should exist");
        let candidate = child_dir.join("artifact.json");
        fs::write(&candidate, "{}").expect("candidate should exist");
        let candidate_canonical = canonicalize_policy_path(&candidate).expect("candidate path");

        let allowed = allowlist_contains_path(&candidate_canonical, &[allow_root])
            .expect("allowlist check should work");

        assert!(allowed);
    }

    #[test]
    fn is_path_allowlisted_returns_false_for_paths_outside_allowlist() {
        let dir = tempdir().expect("tempdir");
        let allow_root = dir.path().join("allow");
        let outside_root = dir.path().join("outside");
        fs::create_dir_all(&allow_root).expect("allow root should exist");
        fs::create_dir_all(&outside_root).expect("outside root should exist");
        let outside_file = outside_root.join("artifact.json");
        fs::write(&outside_file, "{}").expect("outside file should exist");

        let allowed =
            is_path_allowlisted(&outside_file, &[allow_root]).expect("allowlist check should work");

        assert!(!allowed);
    }
}
