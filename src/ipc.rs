use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Serialize;
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, BufReader};

use crate::completion::{CompletionEvent, CompletionParseError, parse_completion_event};
use crate::config::GuardianConfig;

#[derive(Debug, Error)]
pub enum IpcError {
    #[error("ipc_disabled")]
    Disabled,
    #[error("unsupported_request")]
    UnsupportedRequest,
    #[error("serialization_failed: {0}")]
    Serialize(String),
    #[error("bind_failed: {0}")]
    Bind(String),
    #[error("accept_failed: {0}")]
    Accept(String),
    #[error("read_failed: {0}")]
    Read(String),
    #[error("parse_failed: {0}")]
    Parse(String),
    #[error("execution_failed: {0}")]
    Execution(String),
    #[error("unsupported_platform")]
    UnsupportedPlatform,
}

#[derive(Debug, Clone, Serialize)]
struct StatusResponse<'a> {
    status: &'a str,
    mode: &'a str,
}

#[cfg(unix)]
pub struct CompletionEventListener {
    socket_path: PathBuf,
    listener: tokio::net::UnixListener,
}

#[cfg(unix)]
impl CompletionEventListener {
    pub fn bind(socket_path: &Path) -> Result<Self, IpcError> {
        if socket_path.exists() {
            std::fs::remove_file(socket_path).map_err(|error| IpcError::Bind(error.to_string()))?;
        }

        let listener = tokio::net::UnixListener::bind(socket_path)
            .map_err(|error| IpcError::Bind(error.to_string()))?;

        Ok(Self {
            socket_path: socket_path.to_path_buf(),
            listener,
        })
    }

    async fn accept_stream(
        &self,
        idle_timeout: Option<Duration>,
    ) -> Result<Option<tokio::net::UnixStream>, IpcError> {
        match idle_timeout {
            Some(timeout) => match tokio::time::timeout(timeout, self.listener.accept()).await {
                Ok(accepted) => Ok(Some(
                    accepted
                        .map(|(stream, _)| stream)
                        .map_err(|error| IpcError::Accept(error.to_string()))?,
                )),
                Err(_) => Ok(None),
            },
            None => self
                .listener
                .accept()
                .await
                .map(|(stream, _)| Some(stream))
                .map_err(|error| IpcError::Accept(error.to_string())),
        }
    }

    pub async fn receive_batch(
        &self,
        max_events: usize,
        idle_timeout: Option<Duration>,
    ) -> Result<Vec<CompletionEvent>, IpcError> {
        let stream = self.accept_stream(idle_timeout).await?;

        let Some(stream) = stream else {
            return Ok(Vec::new());
        };

        read_event_batch(stream, max_events).await
    }
}

#[cfg(unix)]
async fn read_event_batch(
    stream: tokio::net::UnixStream,
    max_events: usize,
) -> Result<Vec<CompletionEvent>, IpcError> {
    let mut lines = BufReader::new(stream).lines();
    let mut events = Vec::new();

    while events.len() < max_events {
        match lines
            .next_line()
            .await
            .map_err(|error| IpcError::Read(error.to_string()))?
        {
            Some(line) => match parse_batch_line(&line) {
                Ok(Some(event)) => events.push(event),
                Ok(None) => {}
                Err(error) => return Err(error),
            },
            None => break,
        }
    }

    Ok(events)
}

#[cfg(unix)]
impl Drop for CompletionEventListener {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.socket_path);
    }
}

#[cfg(not(unix))]
pub struct CompletionEventListener;

#[cfg(not(unix))]
impl CompletionEventListener {
    pub fn bind(_socket_path: &Path) -> Result<Self, IpcError> {
        Err(IpcError::UnsupportedPlatform)
    }

    pub async fn receive_batch(
        &self,
        _max_events: usize,
        _idle_timeout: Option<Duration>,
    ) -> Result<Vec<CompletionEvent>, IpcError> {
        Err(IpcError::UnsupportedPlatform)
    }
}

pub fn handle_read_only_request(
    config: &GuardianConfig,
    request: &str,
) -> Result<String, IpcError> {
    if !config.ipc.enabled {
        return Err(IpcError::Disabled);
    }

    if request != "status" {
        return Err(IpcError::UnsupportedRequest);
    }

    serde_json::to_string(&StatusResponse {
        status: "ok",
        mode: config.mode.as_str(),
    })
    .map_err(|error| IpcError::Serialize(error.to_string()))
}

#[cfg(unix)]
pub async fn receive_completion_events_once(
    socket_path: &Path,
    max_events: usize,
) -> Result<Vec<CompletionEvent>, IpcError> {
    let listener = CompletionEventListener::bind(socket_path)?;
    listener.receive_batch(max_events, None).await
}

#[cfg(not(unix))]
pub async fn receive_completion_events_once(
    _socket_path: &Path,
    _max_events: usize,
) -> Result<Vec<CompletionEvent>, IpcError> {
    Err(IpcError::UnsupportedPlatform)
}

fn map_parse_error(error: CompletionParseError) -> IpcError {
    IpcError::Parse(error.to_string())
}

fn parse_batch_line(line: &str) -> Result<Option<CompletionEvent>, IpcError> {
    match parse_completion_event(line) {
        Ok(Some(event)) => Ok(Some(event)),
        Ok(None) => Ok(None),
        Err(error) => Err(map_parse_error(error)),
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{CompletionEventListener, IpcError, handle_read_only_request};
    use crate::completion::CompletionSource;
    use crate::config::{GuardianConfig, Mode};

    #[test]
    fn read_only_request_requires_ipc_to_be_enabled() {
        let error = handle_read_only_request(&GuardianConfig::default(), "status")
            .expect_err("ipc disabled should reject requests");

        assert!(matches!(error, IpcError::Disabled));
    }

    #[test]
    fn read_only_request_rejects_unknown_commands() {
        let mut config = GuardianConfig::default();
        config.ipc.enabled = true;

        let error =
            handle_read_only_request(&config, "ping").expect_err("unsupported request should fail");

        assert!(matches!(error, IpcError::UnsupportedRequest));
    }

    #[test]
    fn read_only_request_serializes_current_mode() {
        let mut config = GuardianConfig::default();
        config.ipc.enabled = true;
        config.mode = Mode::Enforce;

        let payload =
            handle_read_only_request(&config, "status").expect("status request should work");

        assert_eq!(payload, r#"{"status":"ok","mode":"enforce"}"#);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn receive_batch_collects_supported_opencode_events() {
        use tempfile::tempdir;
        use tokio::io::AsyncWriteExt;
        use tokio::net::UnixStream;

        let dir = tempdir().expect("tempdir");
        let socket_path = dir.path().join("completion.sock");
        let listener = CompletionEventListener::bind(&socket_path).expect("listener should bind");
        let client_path = socket_path.clone();

        let sender = tokio::spawn(async move {
            let mut stream = UnixStream::connect(&client_path)
                .await
                .expect("client should connect");
            stream
                .write_all(
                    b"{\"type\":\"session.status\",\"event_id\":\"evt-1\",\"session_id\":\"ses-1\",\"status\":\"idle\",\"completed_at\":\"2026-03-08T20:00:00Z\"}\n",
                )
                .await
                .expect("status event should be written");
            stream
                .write_all(
                    b"{\"type\":\"message.created\",\"event_id\":\"evt-2\",\"completed_at\":\"2026-03-08T20:00:01Z\"}\n",
                )
                .await
                .expect("unsupported event should be written");
            stream
                .write_all(
                    b"{\"type\":\"message.part.updated\",\"event_id\":\"evt-3\",\"parent_session_id\":\"ses-parent\",\"child_session_id\":\"ses-child\",\"task_id\":\"task-1\",\"tool_name\":\"write\",\"part_status\":\"completed\",\"completed_at\":\"2026-03-08T20:00:02Z\"}\n",
                )
                .await
                .expect("tool part event should be written");
            stream.shutdown().await.expect("stream should close");
        });

        let events = listener
            .receive_batch(8, Some(Duration::from_secs(1)))
            .await
            .expect("batch should be received");

        sender.await.expect("sender should finish");

        assert_eq!(events.len(), 2);
        assert_eq!(events[0].source, CompletionSource::Status);
        assert_eq!(events[1].source, CompletionSource::ToolPartCompleted);
        assert_eq!(events[1].session_id.as_deref(), Some("ses-child"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn receive_batch_reports_parse_errors() {
        use tempfile::tempdir;
        use tokio::io::AsyncWriteExt;
        use tokio::net::UnixStream;

        let dir = tempdir().expect("tempdir");
        let socket_path = dir.path().join("completion.sock");
        let listener = CompletionEventListener::bind(&socket_path).expect("listener should bind");
        let client_path = socket_path.clone();

        let sender = tokio::spawn(async move {
            let mut stream = UnixStream::connect(&client_path)
                .await
                .expect("client should connect");
            stream
                .write_all(b"{invalid json}\n")
                .await
                .expect("invalid payload should be written");
            stream.shutdown().await.expect("stream should close");
        });

        let error = listener
            .receive_batch(1, Some(Duration::from_secs(1)))
            .await
            .expect_err("invalid payload should fail");

        sender.await.expect("sender should finish");

        assert!(matches!(error, IpcError::Parse(_)));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn receive_batch_returns_empty_on_idle_timeout() {
        use tempfile::tempdir;

        let dir = tempdir().expect("tempdir");
        let socket_path = dir.path().join("completion.sock");
        let listener = CompletionEventListener::bind(&socket_path).expect("listener should bind");

        let events = listener
            .receive_batch(1, Some(Duration::from_millis(10)))
            .await
            .expect("idle timeout should not fail");

        assert!(events.is_empty());
    }
}
