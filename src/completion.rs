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

fn take_required<T>(value: Option<T>, field: &'static str) -> Result<T, CompletionParseError> {
    value.ok_or(CompletionParseError::MissingField(field))
}

fn build_completion_event(
    source: CompletionSource,
    event_id: String,
    session_id: Option<String>,
    parent_session_id: Option<String>,
    task_id: Option<String>,
    tool_name: Option<String>,
    completed_at: String,
) -> CompletionEvent {
    CompletionEvent {
        event_id,
        session_id,
        parent_session_id,
        task_id,
        tool_name,
        completed_at,
        source,
    }
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

    let event_id = take_required(raw.event_id, "event_id")?;
    let completed_at = take_required(raw.completed_at, "completed_at")?;

    match raw.event_type.as_str() {
        "session.status" if raw.status.as_deref() == Some("idle") => {
            Ok(Some(build_completion_event(
                CompletionSource::Status,
                event_id,
                raw.session_id,
                None,
                None,
                None,
                completed_at,
            )))
        }
        "session.idle" => Ok(Some(build_completion_event(
            CompletionSource::Idle,
            event_id,
            raw.session_id,
            None,
            None,
            None,
            completed_at,
        ))),
        "session.error" => Ok(Some(build_completion_event(
            CompletionSource::Error,
            event_id,
            raw.session_id,
            None,
            None,
            None,
            completed_at,
        ))),
        "session.deleted" => Ok(Some(build_completion_event(
            CompletionSource::Deleted,
            event_id,
            raw.session_id,
            None,
            None,
            None,
            completed_at,
        ))),
        "message.part.updated" if raw.part_status.as_deref() == Some("completed") => {
            Ok(Some(build_completion_event(
                CompletionSource::ToolPartCompleted,
                event_id,
                raw.child_session_id,
                raw.parent_session_id,
                raw.task_id,
                raw.tool_name,
                completed_at,
            )))
        }
        _ => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CompletionEvent, CompletionRecordState, CompletionSource, CompletionStateStore,
        CompletionStoreBegin, parse_completion_event,
    };

    fn sample_event(source: CompletionSource) -> CompletionEvent {
        CompletionEvent {
            event_id: "evt-1".to_string(),
            session_id: Some("ses-1".to_string()),
            parent_session_id: Some("ses-parent".to_string()),
            task_id: Some("task-1".to_string()),
            tool_name: Some("write".to_string()),
            completed_at: "2026-03-08T20:00:00Z".to_string(),
            source,
        }
    }

    #[test]
    fn parses_status_idle_event() {
        let event = parse_completion_event(
            r#"{"type":"session.status","event_id":"evt-1","session_id":"ses-1","status":"idle","completed_at":"2026-03-08T20:00:00Z"}"#,
        )
        .expect("status event should parse")
        .expect("status event should map to a completion event");

        assert_eq!(event.source, CompletionSource::Status);
        assert_eq!(event.session_id.as_deref(), Some("ses-1"));
        assert_eq!(event.parent_session_id, None);
    }

    #[test]
    fn parses_tool_part_completed_event() {
        let event = parse_completion_event(
            r#"{"type":"message.part.updated","event_id":"evt-1","parent_session_id":"ses-parent","child_session_id":"ses-child","task_id":"task-1","tool_name":"write","part_status":"completed","completed_at":"2026-03-08T20:00:00Z"}"#,
        )
        .expect("tool part event should parse")
        .expect("tool part event should map to a completion event");

        assert_eq!(event.source, CompletionSource::ToolPartCompleted);
        assert_eq!(event.session_id.as_deref(), Some("ses-child"));
        assert_eq!(event.parent_session_id.as_deref(), Some("ses-parent"));
        assert_eq!(event.task_id.as_deref(), Some("task-1"));
        assert_eq!(event.tool_name.as_deref(), Some("write"));
    }

    #[test]
    fn ignores_unsupported_event_types() {
        let event = parse_completion_event(
            r#"{"type":"message.created","event_id":"evt-1","completed_at":"2026-03-08T20:00:00Z"}"#,
        )
        .expect("unsupported event should still parse");

        assert_eq!(event, None);
    }

    #[test]
    fn rejects_missing_required_fields() {
        let error = parse_completion_event(
            r#"{"type":"session.idle","session_id":"ses-1","completed_at":"2026-03-08T20:00:00Z"}"#,
        )
        .expect_err("missing event_id should fail");

        assert_eq!(error.to_string(), "missing field: event_id");
    }

    #[test]
    fn retries_pending_entries() {
        let mut store = CompletionStateStore::new(60);
        let event = sample_event(CompletionSource::Idle);

        assert_eq!(store.begin(&event, 10), CompletionStoreBegin::Accepted);
        assert_eq!(store.begin(&event, 20), CompletionStoreBegin::RetryPending);
        assert_eq!(store.pending_keys(), vec![event.dedupe_key()]);
    }

    #[test]
    fn skips_processed_duplicates_within_ttl() {
        let mut store = CompletionStateStore::new(60);
        let event = sample_event(CompletionSource::Error);

        assert_eq!(store.begin(&event, 10), CompletionStoreBegin::Accepted);
        store.mark_processed(&event, 20);

        assert_eq!(store.begin(&event, 70), CompletionStoreBegin::SkipDuplicate);
        let snapshot = store.snapshot();
        assert_eq!(snapshot.entries.len(), 1);
        assert_eq!(snapshot.entries[0].state, CompletionRecordState::Processed);
    }

    #[test]
    fn reaccepts_processed_entries_after_ttl() {
        let mut store = CompletionStateStore::new(60);
        let event = sample_event(CompletionSource::Deleted);

        assert_eq!(store.begin(&event, 10), CompletionStoreBegin::Accepted);
        store.mark_processed(&event, 20);

        assert_eq!(store.begin(&event, 81), CompletionStoreBegin::Accepted);
        let snapshot = store.snapshot();
        assert_eq!(snapshot.entries.len(), 1);
        assert_eq!(snapshot.entries[0].state, CompletionRecordState::Pending);
        assert_eq!(snapshot.entries[0].updated_at_unix_secs, 81);
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
