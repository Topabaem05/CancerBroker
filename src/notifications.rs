use std::sync::atomic::{AtomicBool, Ordering};

use notify_rust::Notification;

use crate::remediation::ProcessRemediationOutcome;
use crate::safety::ProcessIdentity;

static NOTIFICATIONS_DISABLED: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemediationReason {
    Leak,
    CompletedSessionCleanup,
}

pub fn notify_process_terminated(
    reason: RemediationReason,
    identity: &ProcessIdentity,
    outcome: &ProcessRemediationOutcome,
    session_id: Option<&str>,
) {
    let Some((summary, body)) = build_process_notification(reason, identity, outcome, session_id)
    else {
        return;
    };
    notify(summary, body);
}

pub fn notify_process_group_terminated(
    reason: RemediationReason,
    pgid: u32,
    leader_identity: &ProcessIdentity,
    outcome: &ProcessRemediationOutcome,
    session_id: Option<&str>,
) {
    let Some((summary, body)) =
        build_group_notification(reason, pgid, leader_identity, outcome, session_id)
    else {
        return;
    };
    notify(summary, body);
}

pub fn send_smoke_notification() -> Result<(), String> {
    let backend = NotifyRustBackend;
    send_smoke_notification_with_backend(&backend)
}

fn notify(summary: String, body: String) {
    if cfg!(test) {
        return;
    }

    let backend = NotifyRustBackend;
    dispatch_notification(&backend, &NOTIFICATIONS_DISABLED, &summary, &body);
}

fn build_process_notification(
    reason: RemediationReason,
    identity: &ProcessIdentity,
    outcome: &ProcessRemediationOutcome,
    session_id: Option<&str>,
) -> Option<(String, String)> {
    if !outcome.was_terminated() {
        return None;
    }

    let summary = match reason {
        RemediationReason::Leak => "CancerBroker terminated a leaking Opencode process",
        RemediationReason::CompletedSessionCleanup => {
            "CancerBroker cleaned up an Opencode process after completion"
        }
    }
    .to_string();

    let mut details = vec![format!("pid {}", identity.pid)];
    if let Some(session_id) = session_id {
        details.push(format!("session {session_id}"));
    }
    if let Some(pgid) = identity.pgid {
        details.push(format!("pgid {pgid}"));
    }
    details.push(termination_label(outcome).to_string());

    Some((summary, details.join(" | ")))
}

fn build_group_notification(
    reason: RemediationReason,
    pgid: u32,
    leader_identity: &ProcessIdentity,
    outcome: &ProcessRemediationOutcome,
    session_id: Option<&str>,
) -> Option<(String, String)> {
    if !outcome.was_terminated() {
        return None;
    }

    let summary = match reason {
        RemediationReason::Leak => "CancerBroker terminated a leaking Opencode process group",
        RemediationReason::CompletedSessionCleanup => {
            "CancerBroker cleaned up an Opencode process group after completion"
        }
    }
    .to_string();

    let mut details = vec![
        format!("pgid {pgid}"),
        format!("leader pid {}", leader_identity.pid),
    ];
    if let Some(session_id) = session_id {
        details.push(format!("session {session_id}"));
    }
    details.push(termination_label(outcome).to_string());

    Some((summary, details.join(" | ")))
}

fn termination_label(outcome: &ProcessRemediationOutcome) -> &'static str {
    match outcome {
        ProcessRemediationOutcome::TerminatedGracefully => "terminated gracefully",
        ProcessRemediationOutcome::TerminatedForced => "terminated forcibly",
        ProcessRemediationOutcome::Rejected(_) | ProcessRemediationOutcome::AlreadyExited => {
            "not terminated"
        }
    }
}

fn send_smoke_notification_with_backend<B: NotificationBackend>(backend: &B) -> Result<(), String> {
    backend.show(
        "CancerBroker notification smoke test",
        "If you can read this, desktop notifications are working.",
    )
}

trait NotificationBackend {
    fn show(&self, summary: &str, body: &str) -> Result<(), String>;
}

struct NotifyRustBackend;

impl NotificationBackend for NotifyRustBackend {
    fn show(&self, summary: &str, body: &str) -> Result<(), String> {
        Notification::new()
            .summary(summary)
            .body(body)
            .show()
            .map(|_| ())
            .map_err(|error| error.to_string())
    }
}

fn dispatch_notification<B: NotificationBackend>(
    backend: &B,
    disabled: &AtomicBool,
    summary: &str,
    body: &str,
) {
    if disabled.load(Ordering::Relaxed) {
        return;
    }

    if backend.show(summary, body).is_err() {
        disabled.store(true, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, Ordering};

    use super::{
        NotificationBackend, RemediationReason, build_group_notification,
        build_process_notification, dispatch_notification, send_smoke_notification_with_backend,
        termination_label,
    };
    use crate::remediation::ProcessRemediationOutcome;
    use crate::safety::ProcessIdentity;

    fn sample_identity() -> ProcessIdentity {
        ProcessIdentity {
            pid: 42,
            parent_pid: Some(7),
            pgid: Some(42),
            start_time_secs: 1,
            uid: Some(501),
            command: "opencode ses_alpha worker".to_string(),
            listening_ports: vec![],
        }
    }

    #[test]
    fn process_notification_skips_non_terminated_outcomes() {
        let identity = sample_identity();
        assert!(
            build_process_notification(
                RemediationReason::Leak,
                &identity,
                &ProcessRemediationOutcome::AlreadyExited,
                Some("ses_alpha"),
            )
            .is_none()
        );
    }

    #[test]
    fn process_notification_mentions_session_and_force_mode() {
        let identity = sample_identity();
        let (_, body) = build_process_notification(
            RemediationReason::CompletedSessionCleanup,
            &identity,
            &ProcessRemediationOutcome::TerminatedForced,
            Some("ses_alpha"),
        )
        .expect("notification");

        assert!(body.contains("pid 42"));
        assert!(body.contains("session ses_alpha"));
        assert!(body.contains("terminated forcibly"));
    }

    #[test]
    fn group_notification_mentions_pgid() {
        let identity = sample_identity();
        let (_, body) = build_group_notification(
            RemediationReason::Leak,
            42,
            &identity,
            &ProcessRemediationOutcome::TerminatedGracefully,
            None,
        )
        .expect("notification");

        assert!(body.contains("pgid 42"));
        assert!(body.contains("leader pid 42"));
    }

    #[test]
    fn termination_labels_match_expected_text() {
        assert_eq!(
            termination_label(&ProcessRemediationOutcome::TerminatedGracefully),
            "terminated gracefully"
        );
        assert_eq!(
            termination_label(&ProcessRemediationOutcome::TerminatedForced),
            "terminated forcibly"
        );
    }

    struct RecordingBackend {
        fail: bool,
        messages: Mutex<Vec<(String, String)>>,
    }

    impl RecordingBackend {
        fn succeed() -> Self {
            Self {
                fail: false,
                messages: Mutex::new(Vec::new()),
            }
        }

        fn fail() -> Self {
            Self {
                fail: true,
                messages: Mutex::new(Vec::new()),
            }
        }
    }

    impl NotificationBackend for RecordingBackend {
        fn show(&self, summary: &str, body: &str) -> Result<(), String> {
            if self.fail {
                return Err("backend unavailable".to_string());
            }
            self.messages
                .lock()
                .expect("messages lock")
                .push((summary.to_string(), body.to_string()));
            Ok(())
        }
    }

    #[test]
    fn dispatch_notification_disables_after_failure() {
        let disabled = AtomicBool::new(false);
        let failing = RecordingBackend::fail();
        let succeeding = RecordingBackend::succeed();

        dispatch_notification(&failing, &disabled, "summary", "body");
        dispatch_notification(&succeeding, &disabled, "summary", "body");

        assert!(disabled.load(Ordering::Relaxed));
        assert!(
            succeeding
                .messages
                .lock()
                .expect("messages lock")
                .is_empty()
        );
    }

    #[test]
    fn smoke_notification_uses_expected_summary_and_body() {
        let backend = RecordingBackend::succeed();

        send_smoke_notification_with_backend(&backend).expect("smoke notification should succeed");

        let messages = backend.messages.lock().expect("messages lock");
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].0, "CancerBroker notification smoke test");
        assert!(messages[0].1.contains("desktop notifications are working"));
    }
}
