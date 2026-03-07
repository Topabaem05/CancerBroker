use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

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
