use std::path::Path;

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
    use tokio::net::UnixListener;

    if socket_path.exists() {
        std::fs::remove_file(socket_path).map_err(|error| IpcError::Bind(error.to_string()))?;
    }

    let listener =
        UnixListener::bind(socket_path).map_err(|error| IpcError::Bind(error.to_string()))?;
    let (stream, _) = listener
        .accept()
        .await
        .map_err(|error| IpcError::Accept(error.to_string()))?;
    let mut lines = BufReader::new(stream).lines();
    let mut events = Vec::new();

    while events.len() < max_events {
        match lines
            .next_line()
            .await
            .map_err(|error| IpcError::Read(error.to_string()))?
        {
            Some(line) => match parse_completion_event(&line) {
                Ok(Some(event)) => events.push(event),
                Ok(None) => {}
                Err(error) => return Err(map_parse_error(error)),
            },
            None => break,
        }
    }

    let _ = std::fs::remove_file(socket_path);
    Ok(events)
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
