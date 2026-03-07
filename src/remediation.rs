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

pub fn remediate_process(
    request: &ProcessRemediationRequest,
) -> Result<ProcessRemediationOutcome, RemediationError> {
    match validate_process_identity(&request.identity, &request.ownership_policy) {
        SafetyDecision::Rejected(reason) => return Ok(ProcessRemediationOutcome::Rejected(reason)),
        SafetyDecision::Allowed => {}
    }

    remediate_process_unix(request)
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

    kill(pid, Some(Signal::SIGTERM))
        .map_err(|error| RemediationError::Signal(error.to_string()))?;

    let start = Instant::now();
    while start.elapsed() <= request.term_timeout {
        if !is_alive_unix(pid) {
            return Ok(ProcessRemediationOutcome::TerminatedGracefully);
        }
        thread::sleep(Duration::from_millis(50));
    }

    kill(pid, Some(Signal::SIGKILL))
        .map_err(|error| RemediationError::Signal(error.to_string()))?;

    Ok(ProcessRemediationOutcome::TerminatedForced)
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

#[cfg(not(unix))]
fn remediate_process_unix(
    _request: &ProcessRemediationRequest,
) -> Result<ProcessRemediationOutcome, RemediationError> {
    Err(RemediationError::UnsupportedPlatform)
}
