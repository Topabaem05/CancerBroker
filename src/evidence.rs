use std::collections::BTreeMap;
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
    let timestamp_unix_secs = now
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);

    PreActionEvidence {
        timestamp_unix_secs,
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

        let file_name = format!(
            "evidence-{}-{}.json",
            evidence.timestamp_unix_secs,
            sanitized_target_id(&evidence.target_id)
        );
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
        Ok(path) => EvidenceWriteOutcome {
            path: Some(path),
            fallback_to_non_destructive: false,
            error: None,
        },
        Err(error) => EvidenceWriteOutcome {
            path: None,
            fallback_to_non_destructive: true,
            error: Some(error.to_string()),
        },
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
