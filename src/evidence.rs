use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceInput {
    pub rationale: String,
    pub prompt_excerpt: Option<String>,
    pub environment: BTreeMap<String, String>,
    pub metadata: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceRecord {
    pub rationale: String,
    pub prompt_excerpt: Option<String>,
    pub environment: BTreeMap<String, String>,
    pub metadata: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignalSnapshot {
    pub name: String,
    pub breached_samples: usize,
    pub window_samples: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreActionEvidence {
    pub timestamp_unix_secs: u64,
    pub target_id: String,
    pub proposed_stage: String,
    pub policy_rationale: String,
    pub signals: Vec<SignalSnapshot>,
    pub redacted_context: EvidenceRecord,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceWriteOutcome {
    pub path: Option<PathBuf>,
    pub fallback_to_non_destructive: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct EvidenceStore {
    root: PathBuf,
}

#[derive(Debug, Error)]
pub enum EvidenceError {
    #[error("evidence io error at {path}: {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },
    #[error("evidence serialization error: {source}")]
    Serialize { source: serde_json::Error },
}

const DEFAULT_EVIDENCE_HOME_RELATIVE_PATH: &str = ".local/share/cancerbroker/evidence";
const DEFAULT_EVIDENCE_FALLBACK_PATH: &str = ".cancerbroker/evidence";

fn build_default_evidence_dir(home: Option<&Path>) -> PathBuf {
    home.map(|path| path.join(DEFAULT_EVIDENCE_HOME_RELATIVE_PATH))
        .unwrap_or_else(|| PathBuf::from(DEFAULT_EVIDENCE_FALLBACK_PATH))
}

pub fn default_evidence_dir() -> PathBuf {
    let home = env::var_os("HOME").map(PathBuf::from);
    build_default_evidence_dir(home.as_deref())
}

fn unix_timestamp_secs(now: SystemTime) -> u64 {
    now.duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn build_evidence_write_success(path: PathBuf) -> EvidenceWriteOutcome {
    EvidenceWriteOutcome {
        path: Some(path),
        fallback_to_non_destructive: false,
        error: None,
    }
}

fn build_evidence_write_failure(error: EvidenceError) -> EvidenceWriteOutcome {
    EvidenceWriteOutcome {
        path: None,
        fallback_to_non_destructive: true,
        error: Some(error.to_string()),
    }
}

pub fn redacted_record(input: EvidenceInput) -> EvidenceRecord {
    let environment = input
        .environment
        .into_keys()
        .map(|key| (key, "[REDACTED]".to_string()))
        .collect();

    EvidenceRecord {
        rationale: input.rationale,
        prompt_excerpt: input.prompt_excerpt.map(|_| "[REDACTED]".to_string()),
        environment,
        metadata: input.metadata,
    }
}

pub fn build_pre_action_evidence(
    now: SystemTime,
    target_id: String,
    proposed_stage: String,
    policy_rationale: String,
    signals: Vec<SignalSnapshot>,
    context: EvidenceInput,
) -> PreActionEvidence {
    PreActionEvidence {
        timestamp_unix_secs: unix_timestamp_secs(now),
        target_id,
        proposed_stage,
        policy_rationale,
        signals,
        redacted_context: redacted_record(context),
    }
}

impl EvidenceStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn persist_pre_action(
        &self,
        evidence: &PreActionEvidence,
    ) -> Result<PathBuf, EvidenceError> {
        fs::create_dir_all(&self.root).map_err(|source| EvidenceError::Io {
            path: self.root.display().to_string(),
            source,
        })?;

        let file_name = pre_action_file_name(evidence);
        let path = self.root.join(file_name);

        let json = serde_json::to_string_pretty(evidence)
            .map_err(|source| EvidenceError::Serialize { source })?;
        fs::write(&path, json).map_err(|source| EvidenceError::Io {
            path: path.display().to_string(),
            source,
        })?;

        Ok(path)
    }
}

pub fn persist_pre_action_with_fallback(
    store: &EvidenceStore,
    evidence: &PreActionEvidence,
) -> EvidenceWriteOutcome {
    match store.persist_pre_action(evidence) {
        Ok(path) => build_evidence_write_success(path),
        Err(error) => build_evidence_write_failure(error),
    }
}

pub fn evidence_exists(path: &Path) -> bool {
    path.exists() && path.is_file()
}

fn sanitized_target_id(target_id: &str) -> String {
    let mut sanitized = String::with_capacity(target_id.len());

    for ch in target_id.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
            sanitized.push(ch);
        } else {
            sanitized.push('_');
        }
    }

    if sanitized.is_empty() {
        "unknown-target".to_string()
    } else {
        sanitized
    }
}

fn pre_action_file_name(evidence: &PreActionEvidence) -> String {
    format!(
        "evidence-{}-{}.json",
        evidence.timestamp_unix_secs,
        sanitized_target_id(&evidence.target_id)
    )
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::time::{Duration, UNIX_EPOCH};

    use tempfile::tempdir;

    use super::{
        EvidenceInput, EvidenceStore, PreActionEvidence, SignalSnapshot,
        build_default_evidence_dir, build_evidence_write_failure, build_evidence_write_success,
        build_pre_action_evidence, default_evidence_dir, evidence_exists,
        persist_pre_action_with_fallback, pre_action_file_name, redacted_record,
        unix_timestamp_secs,
    };

    fn sample_input() -> EvidenceInput {
        EvidenceInput {
            rationale: "step1_warn_throttle".to_string(),
            prompt_excerpt: Some("secret prompt".to_string()),
            environment: [("API_KEY".to_string(), "secret".to_string())]
                .into_iter()
                .collect(),
            metadata: [("session".to_string(), "ses_123".to_string())]
                .into_iter()
                .collect(),
        }
    }

    fn sample_evidence() -> PreActionEvidence {
        build_pre_action_evidence(
            UNIX_EPOCH,
            "cli/target:alpha".to_string(),
            "warn_throttle".to_string(),
            "observe_mode_records_without_action".to_string(),
            vec![SignalSnapshot {
                name: "rss_slope".to_string(),
                breached_samples: 3,
                window_samples: 5,
            }],
            sample_input(),
        )
    }

    #[test]
    fn redacted_record_scrubs_prompt_and_environment_values() {
        let record = redacted_record(sample_input());

        assert_eq!(record.rationale, "step1_warn_throttle");
        assert_eq!(record.prompt_excerpt.as_deref(), Some("[REDACTED]"));
        assert_eq!(
            record.environment.get("API_KEY").map(String::as_str),
            Some("[REDACTED]")
        );
        assert_eq!(
            record.metadata.get("session").map(String::as_str),
            Some("ses_123")
        );
    }

    #[test]
    fn unix_timestamp_secs_clamps_pre_epoch_times() {
        let before_epoch = UNIX_EPOCH
            .checked_sub(Duration::from_secs(1))
            .expect("pre-epoch time should exist");

        assert_eq!(unix_timestamp_secs(before_epoch), 0);
    }

    #[test]
    fn build_pre_action_evidence_redacts_context() {
        let evidence = sample_evidence();

        assert_eq!(evidence.timestamp_unix_secs, 0);
        assert_eq!(evidence.target_id, "cli/target:alpha");
        assert_eq!(evidence.proposed_stage, "warn_throttle");
        assert_eq!(
            evidence.redacted_context.prompt_excerpt.as_deref(),
            Some("[REDACTED]")
        );
        assert_eq!(evidence.signals.len(), 1);
    }

    #[test]
    fn pre_action_file_name_sanitizes_target_ids() {
        let evidence = sample_evidence();

        assert_eq!(
            pre_action_file_name(&evidence),
            "evidence-0-cli_target_alpha.json"
        );
    }

    #[test]
    fn build_default_evidence_dir_prefers_user_data_directory() {
        assert_eq!(
            build_default_evidence_dir(Some(PathBuf::from("/tmp/home").as_path())),
            PathBuf::from("/tmp/home/.local/share/cancerbroker/evidence")
        );
    }

    #[test]
    fn default_evidence_dir_returns_non_empty_path() {
        assert!(!default_evidence_dir().as_os_str().is_empty());
    }

    #[test]
    fn persist_pre_action_writes_json_file() {
        let dir = tempdir().expect("tempdir");
        let store = EvidenceStore::new(dir.path());
        let evidence = sample_evidence();

        let path = store
            .persist_pre_action(&evidence)
            .expect("evidence should persist");

        assert!(evidence_exists(&path));
        let content = fs::read_to_string(&path).expect("written evidence should be readable");
        assert!(content.contains("warn_throttle"));
        assert!(content.contains("[REDACTED]"));
    }

    #[test]
    fn persist_pre_action_with_fallback_marks_failures() {
        let dir = tempdir().expect("tempdir");
        let file_path = dir.path().join("not-a-directory");
        fs::write(&file_path, "occupied").expect("guard file should be written");
        let store = EvidenceStore::new(&file_path);
        let evidence = sample_evidence();

        let outcome = persist_pre_action_with_fallback(&store, &evidence);

        assert_eq!(outcome.path, None);
        assert!(outcome.fallback_to_non_destructive);
        assert!(
            outcome
                .error
                .as_deref()
                .is_some_and(|error| error.contains("evidence io error"))
        );
    }

    #[test]
    fn evidence_write_outcome_builders_match_expected_flags() {
        let success = build_evidence_write_success("/tmp/evidence.json".into());
        assert_eq!(success.path, Some("/tmp/evidence.json".into()));
        assert!(!success.fallback_to_non_destructive);
        assert_eq!(success.error, None);

        let failure = build_evidence_write_failure(super::EvidenceError::Io {
            path: "/tmp/evidence".to_string(),
            source: std::io::Error::other("boom"),
        });
        assert_eq!(failure.path, None);
        assert!(failure.fallback_to_non_destructive);
        assert!(
            failure
                .error
                .as_deref()
                .is_some_and(|error| error.contains("boom"))
        );
    }
}
