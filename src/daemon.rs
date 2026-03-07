use std::path::PathBuf;

use serde::Serialize;

use crate::config::GuardianConfig;
use crate::ipc::{IpcError, receive_completion_events_once};

#[derive(Debug, Clone, Serialize)]
pub struct DaemonOutput {
    pub socket_path: PathBuf,
    pub received_events: usize,
}

pub async fn run_daemon_once(
    config: &GuardianConfig,
    max_events: usize,
) -> Result<DaemonOutput, IpcError> {
    let events =
        receive_completion_events_once(&config.completion.daemon_socket_path, max_events).await?;

    Ok(DaemonOutput {
        socket_path: config.completion.daemon_socket_path.clone(),
        received_events: events.len(),
    })
}
