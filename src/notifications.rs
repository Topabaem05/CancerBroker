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

#[derive(Debug, Clone, Copy, Default)]
pub struct NotificationContext<'a> {
    pub session_id: Option<&'a str>,
    pub execution_path: Option<&'a str>,
    pub leaked_bytes: Option<u64>,
}

pub fn notify_process_terminated(
    reason: RemediationReason,
    identity: &ProcessIdentity,
    outcome: &ProcessRemediationOutcome,
    context: NotificationContext<'_>,
) {
    let Some((summary, body)) = build_process_notification(reason, identity, outcome, context)
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
    context: NotificationContext<'_>,
) {
    let Some((summary, body)) =
        build_group_notification(reason, pgid, leader_identity, outcome, context)
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
    context: NotificationContext<'_>,
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

    Some((
        summary,
        build_notification_body(identity, outcome, context, None),
    ))
}

fn build_group_notification(
    reason: RemediationReason,
    pgid: u32,
    leader_identity: &ProcessIdentity,
    outcome: &ProcessRemediationOutcome,
    context: NotificationContext<'_>,
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

    Some((
        summary,
        build_notification_body(leader_identity, outcome, context, Some(pgid)),
    ))
}

fn build_notification_body(
    identity: &ProcessIdentity,
    outcome: &ProcessRemediationOutcome,
    context: NotificationContext<'_>,
    pgid: Option<u32>,
) -> String {
    let mut lines = Vec::new();

    let mut headline = format!("{} (pid {})", process_name(identity), identity.pid);
    if let Some(session_id) = context.session_id {
        headline.push_str(&format!(", session {session_id}"));
    }
    if let Some(pgid) = pgid {
        headline.push_str(&format!(", pgid {pgid}"));
    }
    lines.push(headline);

    if let Some(path) = execution_path(identity, context.execution_path) {
        lines.push(format!("path {}", shorten_path(&path, 72)));
    }

    lines.push(memory_line(
        identity.current_rss_bytes,
        context.leaked_bytes,
    ));
    lines.push(termination_label(outcome).to_string());
    lines.join("\n")
}

fn process_name(identity: &ProcessIdentity) -> String {
    let token = command_token(&identity.command);
    token
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(token)
        .to_string()
}

fn execution_path(identity: &ProcessIdentity, explicit: Option<&str>) -> Option<String> {
    if let Some(explicit) = explicit
        && !explicit.is_empty()
    {
        return Some(explicit.to_string());
    }

    let token = command_token(&identity.command);
    if token.contains('/') || token.contains('\\') {
        Some(token.to_string())
    } else {
        None
    }
}

fn command_token(command: &str) -> &str {
    command.split_whitespace().next().unwrap_or(command)
}

fn shorten_path(path: &str, max_chars: usize) -> String {
    let char_count = path.chars().count();
    if char_count <= max_chars {
        return path.to_string();
    }

    let keep = max_chars.saturating_sub(1);
    let tail = path
        .chars()
        .rev()
        .take(keep)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<String>();
    format!("…{tail}")
}

fn memory_line(current_rss_bytes: u64, leaked_bytes: Option<u64>) -> String {
    match leaked_bytes {
        Some(leaked_bytes) => format!(
            "leaked {} | rss {}",
            human_bytes(leaked_bytes),
            human_bytes(current_rss_bytes)
        ),
        None => format!("rss {}", human_bytes(current_rss_bytes)),
    }
}

fn human_bytes(bytes: u64) -> String {
    const KIB: u64 = 1024;
    const MIB: u64 = KIB * 1024;
    const GIB: u64 = MIB * 1024;

    if bytes >= GIB {
        format!("{:.1} GiB", bytes as f64 / GIB as f64)
    } else if bytes >= MIB {
        format!("{:.0} MiB", bytes as f64 / MIB as f64)
    } else if bytes >= KIB {
        format!("{:.0} KiB", bytes as f64 / KIB as f64)
    } else {
        format!("{bytes} B")
    }
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
        NotificationBackend, NotificationContext, RemediationReason, build_group_notification,
        build_process_notification, dispatch_notification, human_bytes,
        send_smoke_notification_with_backend, termination_label,
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
            current_rss_bytes: 512 * 1024 * 1024,
            command: "/tmp/project/opencode-worker ses_alpha".to_string(),
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
                NotificationContext {
                    session_id: Some("ses_alpha"),
                    ..NotificationContext::default()
                },
            )
            .is_none()
        );
    }

    #[test]
    fn process_notification_mentions_session_path_and_memory() {
        let identity = sample_identity();
        let (_, body) = build_process_notification(
            RemediationReason::CompletedSessionCleanup,
            &identity,
            &ProcessRemediationOutcome::TerminatedForced,
            NotificationContext {
                session_id: Some("ses_alpha"),
                execution_path: Some("/tmp/project/opencode-worker"),
                leaked_bytes: Some(64 * 1024 * 1024),
            },
        )
        .expect("notification");

        assert!(body.contains("opencode-worker (pid 42), session ses_alpha"));
        assert!(body.contains("path /tmp/project/opencode-worker"));
        assert!(body.contains("leaked 64 MiB | rss 512 MiB"));
        assert!(body.contains("terminated forcibly"));
    }

    #[test]
    fn group_notification_mentions_pgid_and_process_name() {
        let identity = sample_identity();
        let (_, body) = build_group_notification(
            RemediationReason::Leak,
            42,
            &identity,
            &ProcessRemediationOutcome::TerminatedGracefully,
            NotificationContext::default(),
        )
        .expect("notification");

        assert!(body.contains("opencode-worker (pid 42), pgid 42"));
        assert!(body.contains("rss 512 MiB"));
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

    #[test]
    fn human_bytes_formats_expected_units() {
        assert_eq!(human_bytes(512), "512 B");
        assert_eq!(human_bytes(2 * 1024), "2 KiB");
        assert_eq!(human_bytes(64 * 1024 * 1024), "64 MiB");
        assert_eq!(human_bytes(3 * 1024 * 1024 * 1024), "3.0 GiB");
    }
}
