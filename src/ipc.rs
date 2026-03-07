use serde::Serialize;
use thiserror::Error;

use crate::config::GuardianConfig;

#[derive(Debug, Error)]
pub enum IpcError {
    #[error("ipc_disabled")]
    Disabled,
    #[error("unsupported_request")]
    UnsupportedRequest,
    #[error("serialization_failed: {0}")]
    Serialize(String),
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
