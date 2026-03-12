use std::thread;
use std::time::{Duration, Instant};

use thiserror::Error;

use crate::safety::{OwnershipPolicy, ProcessIdentity, SafetyDecision, validate_process_identity};

#[derive(Debug, Clone)]
pub struct ProcessRemediationRequest {
    pub identity: ProcessIdentity,
    pub ownership_policy: OwnershipPolicy,
    pub term_timeout: Duration,
}

#[derive(Debug, Clone)]
pub struct ProcessGroupRemediationRequest {
    pub pgid: u32,
    pub leader_identity: ProcessIdentity,
    pub ownership_policy: OwnershipPolicy,
    pub term_timeout: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProcessRemediationOutcome {
    Rejected(&'static str),
    AlreadyExited,
    TerminatedGracefully,
    TerminatedForced,
}

#[derive(Debug, Error)]
pub enum RemediationError {
    #[error("unsupported platform")]
    UnsupportedPlatform,
    #[error("signal failure: {0}")]
    Signal(String),
}

fn signal_error(error: impl ToString) -> RemediationError {
    RemediationError::Signal(error.to_string())
}

#[cfg(unix)]
fn wait_for_exit(pid: nix::unistd::Pid, timeout: Duration) -> bool {
    let start = Instant::now();
    while start.elapsed() <= timeout {
        if !is_alive_unix(pid) {
            return true;
        }
        thread::sleep(Duration::from_millis(50));
    }

    false
}

pub fn remediate_process(
    request: &ProcessRemediationRequest,
) -> Result<ProcessRemediationOutcome, RemediationError> {
    match validate_process_identity(&request.identity, &request.ownership_policy) {
        SafetyDecision::Rejected(reason) => return Ok(ProcessRemediationOutcome::Rejected(reason)),
        SafetyDecision::Allowed => {}
    }

    remediate_process_unix(request)
}

pub fn remediate_process_group(
    request: &ProcessGroupRemediationRequest,
) -> Result<ProcessRemediationOutcome, RemediationError> {
    match validate_process_identity(&request.leader_identity, &request.ownership_policy) {
        SafetyDecision::Rejected(reason) => return Ok(ProcessRemediationOutcome::Rejected(reason)),
        SafetyDecision::Allowed => {}
    }

    remediate_process_group_unix(request)
}

#[cfg(unix)]
fn remediate_process_unix(
    request: &ProcessRemediationRequest,
) -> Result<ProcessRemediationOutcome, RemediationError> {
    use nix::sys::signal::{Signal, kill};
    use nix::unistd::Pid;

    let pid = Pid::from_raw(request.identity.pid as i32);

    if !is_alive_unix(pid) {
        return Ok(ProcessRemediationOutcome::AlreadyExited);
    }

    kill(pid, Some(Signal::SIGTERM)).map_err(signal_error)?;

    if wait_for_exit(pid, request.term_timeout) {
        return Ok(ProcessRemediationOutcome::TerminatedGracefully);
    }

    kill(pid, Some(Signal::SIGKILL)).map_err(signal_error)?;

    Ok(ProcessRemediationOutcome::TerminatedForced)
}

#[cfg(not(unix))]
fn remediate_process_unix(
    _request: &ProcessRemediationRequest,
) -> Result<ProcessRemediationOutcome, RemediationError> {
    Err(RemediationError::UnsupportedPlatform)
}

#[cfg(unix)]
fn is_alive_unix(pid: nix::unistd::Pid) -> bool {
    use nix::errno::Errno;
    use nix::sys::signal::kill;

    match kill(pid, None) {
        Ok(()) => true,
        Err(Errno::ESRCH) => false,
        Err(_) => true,
    }
}

#[cfg(unix)]
fn is_process_group_alive(pgid: nix::unistd::Pid) -> bool {
    use nix::errno::Errno;
    use nix::sys::signal::killpg;

    match killpg(pgid, None) {
        Ok(()) => true,
        Err(Errno::ESRCH) => false,
        Err(_) => true,
    }
}

#[cfg(unix)]
fn wait_for_group_exit(pgid: nix::unistd::Pid, timeout: Duration) -> bool {
    let start = Instant::now();
    while start.elapsed() <= timeout {
        if !is_process_group_alive(pgid) {
            return true;
        }
        thread::sleep(Duration::from_millis(50));
    }
    false
}

#[cfg(unix)]
fn remediate_process_group_unix(
    request: &ProcessGroupRemediationRequest,
) -> Result<ProcessRemediationOutcome, RemediationError> {
    use nix::sys::signal::{Signal, killpg};
    use nix::unistd::Pid;

    let pgid = Pid::from_raw(request.pgid as i32);

    if !is_process_group_alive(pgid) {
        return Ok(ProcessRemediationOutcome::AlreadyExited);
    }

    killpg(pgid, Some(Signal::SIGTERM)).map_err(signal_error)?;

    if wait_for_group_exit(pgid, request.term_timeout) {
        return Ok(ProcessRemediationOutcome::TerminatedGracefully);
    }

    killpg(pgid, Some(Signal::SIGKILL)).map_err(signal_error)?;

    Ok(ProcessRemediationOutcome::TerminatedForced)
}

#[cfg(not(unix))]
fn remediate_process_group_unix(
    _request: &ProcessGroupRemediationRequest,
) -> Result<ProcessRemediationOutcome, RemediationError> {
    Err(RemediationError::UnsupportedPlatform)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{
        ProcessGroupRemediationRequest, ProcessRemediationOutcome, ProcessRemediationRequest,
        remediate_process, remediate_process_group, signal_error,
    };
    use crate::platform::current_effective_uid;
    use crate::safety::{OwnershipPolicy, ProcessIdentity};

    fn sample_identity() -> ProcessIdentity {
        ProcessIdentity {
            pid: std::process::id(),
            parent_pid: None,
            pgid: None,
            start_time_secs: 0,
            uid: Some(current_effective_uid()),
            command: "opencode worker".to_string(),
            listening_ports: vec![],
        }
    }

    fn sample_policy() -> OwnershipPolicy {
        OwnershipPolicy {
            expected_uid: current_effective_uid(),
            required_command_markers: vec!["opencode".to_string()],
            same_uid_only: true,
        }
    }

    fn sample_request() -> ProcessRemediationRequest {
        ProcessRemediationRequest {
            identity: sample_identity(),
            ownership_policy: sample_policy(),
            term_timeout: Duration::from_millis(1),
        }
    }

    #[test]
    fn remediate_process_rejects_uid_mismatch_before_signals() {
        let mut request = sample_request();
        request.identity.uid = Some(request.ownership_policy.expected_uid + 1);

        let outcome = remediate_process(&request).expect("uid mismatch should not error");

        assert_eq!(outcome, ProcessRemediationOutcome::Rejected("uid_mismatch"));
    }

    #[test]
    fn remediate_process_rejects_command_marker_mismatch_before_signals() {
        let mut request = sample_request();
        request.identity.command = "bash worker".to_string();

        let outcome = remediate_process(&request).expect("command mismatch should not error");

        assert_eq!(
            outcome,
            ProcessRemediationOutcome::Rejected("command_marker_mismatch")
        );
    }

    #[test]
    fn signal_error_wraps_displayable_messages() {
        let error = signal_error("boom");

        assert_eq!(error.to_string(), "signal failure: boom");
    }

    #[cfg(unix)]
    #[test]
    fn is_alive_unix_reports_current_process_as_alive() {
        let pid = nix::unistd::Pid::from_raw(std::process::id() as i32);

        assert!(super::is_alive_unix(pid));
    }

    #[cfg(unix)]
    #[test]
    fn is_alive_unix_reports_missing_process_as_not_alive() {
        let pid = nix::unistd::Pid::from_raw(999_999);

        assert!(!super::is_alive_unix(pid));
    }

    #[cfg(unix)]
    #[test]
    fn is_process_group_alive_reports_own_group_as_alive() {
        let pgid = nix::unistd::getpgrp();

        assert!(super::is_process_group_alive(pgid));
    }

    #[cfg(unix)]
    #[test]
    fn is_process_group_alive_reports_missing_group_as_not_alive() {
        let pgid = nix::unistd::Pid::from_raw(999_999);

        assert!(!super::is_process_group_alive(pgid));
    }

    #[test]
    fn remediate_process_group_rejects_uid_mismatch() {
        let mut identity = sample_identity();
        identity.uid = Some(sample_policy().expected_uid + 1);

        let request = ProcessGroupRemediationRequest {
            pgid: 99999,
            leader_identity: identity,
            ownership_policy: sample_policy(),
            term_timeout: Duration::from_millis(1),
        };

        let outcome = remediate_process_group(&request).expect("uid mismatch should not error");

        assert_eq!(outcome, ProcessRemediationOutcome::Rejected("uid_mismatch"));
    }

    #[test]
    fn remediate_process_group_rejects_command_marker_mismatch() {
        let mut identity = sample_identity();
        identity.command = "bash worker".to_string();

        let request = ProcessGroupRemediationRequest {
            pgid: 99999,
            leader_identity: identity,
            ownership_policy: sample_policy(),
            term_timeout: Duration::from_millis(1),
        };

        let outcome = remediate_process_group(&request).expect("command mismatch should not error");

        assert_eq!(
            outcome,
            ProcessRemediationOutcome::Rejected("command_marker_mismatch")
        );
    }

    #[cfg(unix)]
    #[test]
    fn remediate_process_group_reports_already_exited_for_missing_group() {
        let request = ProcessGroupRemediationRequest {
            pgid: 999_999,
            leader_identity: sample_identity(),
            ownership_policy: sample_policy(),
            term_timeout: Duration::from_millis(1),
        };

        let outcome = remediate_process_group(&request).expect("missing group should not error");

        assert_eq!(outcome, ProcessRemediationOutcome::AlreadyExited);
    }
}
