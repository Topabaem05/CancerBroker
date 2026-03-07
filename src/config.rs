use std::fs;
use std::path::Path;

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
pub struct GuardianConfig {
    #[serde(default)]
    pub mode: Mode,
}

impl Default for GuardianConfig {
    fn default() -> Self {
        Self {
            mode: Mode::Observe,
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
