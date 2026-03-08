use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompletionSource {
    Status,
    Idle,
    ToolPartCompleted,
    Error,
    Deleted,
    Inferred,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompletionEvent {
    pub event_id: String,
    pub session_id: Option<String>,
    pub parent_session_id: Option<String>,
    pub task_id: Option<String>,
    pub tool_name: Option<String>,
    pub completed_at: String,
    pub source: CompletionSource,
}

impl CompletionEvent {
    pub fn dedupe_key(&self) -> String {
        format!(
            "{}:{}:{}:{}",
            self.event_id,
            self.session_id.as_deref().unwrap_or("-"),
            self.task_id.as_deref().unwrap_or("-"),
            self.source.as_str()
        )
    }
}

impl CompletionSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Status => "status",
            Self::Idle => "idle",
            Self::ToolPartCompleted => "tool_part_completed",
            Self::Error => "error",
            Self::Deleted => "deleted",
            Self::Inferred => "inferred",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompletionStateRecord {
    pub dedupe_key: String,
    pub processed_at_unix_secs: u64,
}

impl CompletionStateRecord {
    pub fn from_event(event: &CompletionEvent, processed_at_unix_secs: u64) -> Self {
        Self {
            dedupe_key: event.dedupe_key(),
            processed_at_unix_secs,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompletionRecordState {
    Pending,
    Processed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompletionStateEntry {
    pub dedupe_key: String,
    pub updated_at_unix_secs: u64,
    pub state: CompletionRecordState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct CompletionStateSnapshot {
    pub entries: Vec<CompletionStateEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompletionStoreBegin {
    Accepted,
    RetryPending,
    SkipDuplicate,
}

#[derive(Debug, Clone, Default)]
pub struct CompletionStateStore {
    dedupe_ttl_secs: u64,
    entries: BTreeMap<String, CompletionStateEntry>,
}

impl CompletionStateStore {
    pub fn new(dedupe_ttl_secs: u64) -> Self {
        Self {
            dedupe_ttl_secs,
            entries: BTreeMap::new(),
        }
    }

    pub fn from_snapshot(dedupe_ttl_secs: u64, snapshot: CompletionStateSnapshot) -> Self {
        let entries = snapshot
            .entries
            .into_iter()
            .map(|entry| (entry.dedupe_key.clone(), entry))
            .collect();

        Self {
            dedupe_ttl_secs,
            entries,
        }
    }

    pub fn snapshot(&self) -> CompletionStateSnapshot {
        CompletionStateSnapshot {
            entries: self.entries.values().cloned().collect(),
        }
    }

    pub fn begin(&mut self, event: &CompletionEvent, now_unix_secs: u64) -> CompletionStoreBegin {
        let key = event.dedupe_key();

        if let Some(entry) = self.entries.get_mut(&key) {
            return match entry.state {
                CompletionRecordState::Pending => {
                    entry.updated_at_unix_secs = now_unix_secs;
                    CompletionStoreBegin::RetryPending
                }
                CompletionRecordState::Processed
                    if now_unix_secs.saturating_sub(entry.updated_at_unix_secs)
                        <= self.dedupe_ttl_secs =>
                {
                    CompletionStoreBegin::SkipDuplicate
                }
                CompletionRecordState::Processed => {
                    entry.updated_at_unix_secs = now_unix_secs;
                    entry.state = CompletionRecordState::Pending;
                    CompletionStoreBegin::Accepted
                }
            };
        }

        self.entries.insert(
            key.clone(),
            CompletionStateEntry {
                dedupe_key: key,
                updated_at_unix_secs: now_unix_secs,
                state: CompletionRecordState::Pending,
            },
        );
        CompletionStoreBegin::Accepted
    }

    pub fn mark_processed(&mut self, event: &CompletionEvent, now_unix_secs: u64) {
        let key = event.dedupe_key();
        self.entries.insert(
            key.clone(),
            CompletionStateEntry {
                dedupe_key: key,
                updated_at_unix_secs: now_unix_secs,
                state: CompletionRecordState::Processed,
            },
        );
    }

    pub fn pending_keys(&self) -> Vec<String> {
        self.entries
            .values()
            .filter(|entry| entry.state == CompletionRecordState::Pending)
            .map(|entry| entry.dedupe_key.clone())
            .collect()
    }
}

#[derive(Debug, Error)]
pub enum CompletionParseError {
    #[error("invalid json: {0}")]
    Json(String),
    #[error("missing field: {0}")]
    MissingField(&'static str),
}

#[derive(Debug, Error)]
pub enum CompletionStateIoError {
    #[error("completion state io error at {path}: {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },
    #[error("completion state parse error at {path}: {source}")]
    Parse {
        path: String,
        source: serde_json::Error,
    },
    #[error("completion state serialize error: {0}")]
    Serialize(serde_json::Error),
}

#[derive(Debug, Deserialize)]
struct RawCompletionEvent {
    #[serde(rename = "type")]
    event_type: String,
    event_id: Option<String>,
    session_id: Option<String>,
    parent_session_id: Option<String>,
    child_session_id: Option<String>,
    task_id: Option<String>,
    tool_name: Option<String>,
    status: Option<String>,
    part_status: Option<String>,
    completed_at: Option<String>,
}

pub fn parse_completion_event(line: &str) -> Result<Option<CompletionEvent>, CompletionParseError> {
    let raw: RawCompletionEvent = serde_json::from_str(line)
        .map_err(|error| CompletionParseError::Json(error.to_string()))?;

    let event_id = raw
        .event_id
        .ok_or(CompletionParseError::MissingField("event_id"))?;
    let completed_at = raw
        .completed_at
        .ok_or(CompletionParseError::MissingField("completed_at"))?;

    match raw.event_type.as_str() {
        "session.status" if raw.status.as_deref() == Some("idle") => Ok(Some(CompletionEvent {
            event_id,
            session_id: raw.session_id,
            parent_session_id: None,
            task_id: None,
            tool_name: None,
            completed_at,
            source: CompletionSource::Status,
        })),
        "session.idle" => Ok(Some(CompletionEvent {
            event_id,
            session_id: raw.session_id,
            parent_session_id: None,
            task_id: None,
            tool_name: None,
            completed_at,
            source: CompletionSource::Idle,
        })),
        "session.error" => Ok(Some(CompletionEvent {
            event_id,
            session_id: raw.session_id,
            parent_session_id: None,
            task_id: None,
            tool_name: None,
            completed_at,
            source: CompletionSource::Error,
        })),
        "session.deleted" => Ok(Some(CompletionEvent {
            event_id,
            session_id: raw.session_id,
            parent_session_id: None,
            task_id: None,
            tool_name: None,
            completed_at,
            source: CompletionSource::Deleted,
        })),
        "message.part.updated" if raw.part_status.as_deref() == Some("completed") => {
            Ok(Some(CompletionEvent {
                event_id,
                session_id: raw.child_session_id,
                parent_session_id: raw.parent_session_id,
                task_id: raw.task_id,
                tool_name: raw.tool_name,
                completed_at,
                source: CompletionSource::ToolPartCompleted,
            }))
        }
        _ => Ok(None),
    }
}

pub fn load_completion_state(
    path: &Path,
    dedupe_ttl_secs: u64,
) -> Result<CompletionStateStore, CompletionStateIoError> {
    if !path.exists() {
        return Ok(CompletionStateStore::new(dedupe_ttl_secs));
    }

    let content = fs::read_to_string(path).map_err(|source| CompletionStateIoError::Io {
        path: path.display().to_string(),
        source,
    })?;
    let snapshot: CompletionStateSnapshot =
        serde_json::from_str(&content).map_err(|source| CompletionStateIoError::Parse {
            path: path.display().to_string(),
            source,
        })?;

    Ok(CompletionStateStore::from_snapshot(
        dedupe_ttl_secs,
        snapshot,
    ))
}

pub fn persist_completion_state(
    path: &Path,
    store: &CompletionStateStore,
) -> Result<(), CompletionStateIoError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| CompletionStateIoError::Io {
            path: parent.display().to_string(),
            source,
        })?;
    }

    let json = serde_json::to_string_pretty(&store.snapshot())
        .map_err(CompletionStateIoError::Serialize)?;
    fs::write(path, json).map_err(|source| CompletionStateIoError::Io {
        path: path.display().to_string(),
        source,
    })
}
