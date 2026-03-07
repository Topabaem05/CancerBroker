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

#[derive(Debug, Error)]
pub enum CompletionParseError {
    #[error("invalid json: {0}")]
    Json(String),
    #[error("missing field: {0}")]
    MissingField(&'static str),
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
