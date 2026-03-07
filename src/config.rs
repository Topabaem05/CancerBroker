use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    #[default]
    Observe,
    Enforce,
}

impl Mode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Observe => "observe",
            Self::Enforce => "enforce",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SamplingPolicy {
    pub sample_interval_secs: u64,
    pub breach_window_samples: usize,
    pub breach_required_samples: usize,
    pub signal_quorum: usize,
    pub active_session_grace_minutes: u64,
}

impl Default for SamplingPolicy {
    fn default() -> Self {
        Self {
            sample_interval_secs: 5,
            breach_window_samples: 5,
            breach_required_samples: 3,
            signal_quorum: 2,
            active_session_grace_minutes: 10,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DetectionThresholds {
    pub rss_slope_mib_per_min: u64,
    pub rss_slope_duration_minutes: u64,
    pub orphan_count: usize,
    pub stale_artifact_growth_gib: u64,
}

impl Default for DetectionThresholds {
    fn default() -> Self {
        Self {
            rss_slope_mib_per_min: 200,
            rss_slope_duration_minutes: 5,
            orphan_count: 3,
            stale_artifact_growth_gib: 5,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionBudget {
    pub max_destructive_per_target_per_hour: u32,
    pub max_destructive_per_day: u32,
}

impl Default for ActionBudget {
    fn default() -> Self {
        Self {
            max_destructive_per_target_per_hour: 1,
            max_destructive_per_day: 3,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoragePolicy {
    pub allowlist: Vec<PathBuf>,
}

impl Default for StoragePolicy {
    fn default() -> Self {
        Self {
            allowlist: vec![PathBuf::from("~/.local/share/opencode/storage")],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceRetention {
    pub days: u64,
    pub max_mib: u64,
}

impl Default for EvidenceRetention {
    fn default() -> Self {
        Self {
            days: 7,
            max_mib: 500,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SafetyPolicy {
    pub same_uid_only: bool,
    pub required_command_markers: Vec<String>,
}

impl Default for SafetyPolicy {
    fn default() -> Self {
        Self {
            same_uid_only: true,
            required_command_markers: vec!["opencode".to_string(), "openagent".to_string()],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuardianConfig {
    #[serde(default)]
    pub mode: Mode,
    #[serde(default)]
    pub sampling: SamplingPolicy,
    #[serde(default)]
    pub thresholds: DetectionThresholds,
    #[serde(default)]
    pub budgets: ActionBudget,
    #[serde(default)]
    pub storage: StoragePolicy,
    #[serde(default)]
    pub evidence_retention: EvidenceRetention,
    #[serde(default)]
    pub safety: SafetyPolicy,
}

impl Default for GuardianConfig {
    fn default() -> Self {
        Self {
            mode: Mode::Observe,
            sampling: SamplingPolicy::default(),
            thresholds: DetectionThresholds::default(),
            budgets: ActionBudget::default(),
            storage: StoragePolicy::default(),
            evidence_retention: EvidenceRetention::default(),
            safety: SafetyPolicy::default(),
        }
    }
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("config read error at {path}: {source}")]
    Read {
        path: String,
        source: std::io::Error,
    },
    #[error("config parse error at {path}: {source}")]
    Parse {
        path: String,
        source: toml::de::Error,
    },
}

pub fn load_config(path: &Path) -> Result<GuardianConfig, ConfigError> {
    let content = fs::read_to_string(path).map_err(|source| ConfigError::Read {
        path: path.display().to_string(),
        source,
    })?;

    toml::from_str(&content).map_err(|source| ConfigError::Parse {
        path: path.display().to_string(),
        source,
    })
}
