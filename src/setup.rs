use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use color_eyre::eyre::Result;
use serde_json::{Map, Value, json};
use thiserror::Error;

const OPENCODE_CONFIG_RELATIVE_PATH: &str = ".config/opencode/opencode.json";
const CANCERBROKER_MCP_KEY: &str = "cancerbroker";
const CANCERBROKER_MCP_TIMEOUT_MS: u64 = 30_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetupOutcome {
    pub config_path: PathBuf,
    pub backup_path: Option<PathBuf>,
    pub installed: bool,
}

#[derive(Debug, Error)]
pub enum SetupError {
    #[error("HOME is not set")]
    MissingHome,
    #[error("opencode config read error at {path}: {source}")]
    Read {
        path: String,
        source: std::io::Error,
    },
    #[error("opencode config parse error at {path}: {source}")]
    Parse {
        path: String,
        source: serde_json::Error,
    },
    #[error("opencode config write error at {path}: {source}")]
    Write {
        path: String,
        source: std::io::Error,
    },
    #[error("opencode config root must be a JSON object")]
    InvalidRoot,
    #[error("opencode mcp section must be a JSON object")]
    InvalidMcpSection,
}

fn home_dir() -> Option<PathBuf> {
    env::var_os("HOME").map(PathBuf::from)
}

fn opencode_config_path(home: &Path) -> PathBuf {
    home.join(OPENCODE_CONFIG_RELATIVE_PATH)
}

fn backup_path(config_path: &Path) -> PathBuf {
    config_path.with_extension("json.bak")
}

fn cancerbroker_mcp_entry() -> Value {
    json!({
        "type": "local",
        "command": ["cancerbroker", "mcp"],
        "enabled": true,
        "timeout": CANCERBROKER_MCP_TIMEOUT_MS,
    })
}

fn read_opencode_config(config_path: &Path) -> Result<Value, SetupError> {
    if !config_path.exists() {
        return Ok(Value::Object(Map::new()));
    }

    let content = fs::read_to_string(config_path).map_err(|source| SetupError::Read {
        path: config_path.display().to_string(),
        source,
    })?;

    serde_json::from_str(&content).map_err(|source| SetupError::Parse {
        path: config_path.display().to_string(),
        source,
    })
}

fn ensure_object(value: &mut Value) -> Result<&mut Map<String, Value>, SetupError> {
    value.as_object_mut().ok_or(SetupError::InvalidRoot)
}

fn ensure_mcp_object(root: &mut Value) -> Result<&mut Map<String, Value>, SetupError> {
    let root = ensure_object(root)?;
    let mcp = root
        .entry("mcp".to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    mcp.as_object_mut().ok_or(SetupError::InvalidMcpSection)
}

fn update_opencode_config(root: &mut Value, install: bool) -> Result<(), SetupError> {
    let mcp = ensure_mcp_object(root)?;
    if install {
        mcp.insert(CANCERBROKER_MCP_KEY.to_string(), cancerbroker_mcp_entry());
    } else {
        mcp.remove(CANCERBROKER_MCP_KEY);
    }
    Ok(())
}

fn write_backup_if_present(config_path: &Path) -> Result<Option<PathBuf>, SetupError> {
    if !config_path.exists() {
        return Ok(None);
    }

    let backup_path = backup_path(config_path);
    fs::copy(config_path, &backup_path).map_err(|source| SetupError::Write {
        path: backup_path.display().to_string(),
        source,
    })?;
    Ok(Some(backup_path))
}

fn write_opencode_config(config_path: &Path, root: &Value) -> Result<(), SetupError> {
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent).map_err(|source| SetupError::Write {
            path: parent.display().to_string(),
            source,
        })?;
    }

    let content = serde_json::to_string_pretty(root).map_err(|source| SetupError::Parse {
        path: config_path.display().to_string(),
        source,
    })?;
    fs::write(config_path, format!("{content}\n")).map_err(|source| SetupError::Write {
        path: config_path.display().to_string(),
        source,
    })?;
    Ok(())
}

fn configure_opencode(config_path: &Path, install: bool) -> Result<SetupOutcome, SetupError> {
    if !install && !config_path.exists() {
        return Ok(SetupOutcome {
            config_path: config_path.to_path_buf(),
            backup_path: None,
            installed: false,
        });
    }

    let mut root = read_opencode_config(config_path)?;
    let backup_path = write_backup_if_present(config_path)?;
    update_opencode_config(&mut root, install)?;
    write_opencode_config(config_path, &root)?;

    Ok(SetupOutcome {
        config_path: config_path.to_path_buf(),
        backup_path,
        installed: install,
    })
}

pub fn setup() -> Result<SetupOutcome> {
    let home = home_dir().ok_or(SetupError::MissingHome)?;
    configure_opencode(&opencode_config_path(&home), true).map_err(Into::into)
}

pub fn uninstall() -> Result<SetupOutcome> {
    let home = home_dir().ok_or(SetupError::MissingHome)?;
    configure_opencode(&opencode_config_path(&home), false).map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use serde_json::json;
    use tempfile::tempdir;

    use super::{
        CANCERBROKER_MCP_KEY, backup_path, cancerbroker_mcp_entry, configure_opencode,
        opencode_config_path, update_opencode_config,
    };

    #[test]
    fn opencode_config_path_joins_home_directory() {
        assert_eq!(
            opencode_config_path(PathBuf::from("/tmp/home").as_path()),
            PathBuf::from("/tmp/home/.config/opencode/opencode.json")
        );
    }

    #[test]
    fn backup_path_keeps_json_suffix() {
        assert_eq!(
            backup_path(PathBuf::from("/tmp/opencode.json").as_path()),
            PathBuf::from("/tmp/opencode.json.bak")
        );
    }

    #[test]
    fn update_opencode_config_adds_cancerbroker_entry() {
        let mut root = json!({
            "mcp": {
                "sequential-thinking": {
                    "type": "local"
                }
            }
        });

        update_opencode_config(&mut root, true).expect("setup update");

        assert_eq!(root["mcp"][CANCERBROKER_MCP_KEY], cancerbroker_mcp_entry());
        assert_eq!(root["mcp"]["sequential-thinking"]["type"], "local");
    }

    #[test]
    fn update_opencode_config_removes_cancerbroker_entry() {
        let mut root = json!({
            "mcp": {
                "cancerbroker": cancerbroker_mcp_entry(),
                "other": {
                    "type": "remote"
                }
            }
        });

        update_opencode_config(&mut root, false).expect("uninstall update");

        assert!(root["mcp"].get(CANCERBROKER_MCP_KEY).is_none());
        assert_eq!(root["mcp"]["other"]["type"], "remote");
    }

    #[test]
    fn configure_opencode_creates_backup_and_writes_setup_entry() {
        let dir = tempdir().expect("tempdir");
        let config_path = dir.path().join("opencode.json");
        fs::write(&config_path, "{\"mcp\":{}}\n").expect("config file");

        let outcome = configure_opencode(&config_path, true).expect("setup outcome");
        let content = fs::read_to_string(&config_path).expect("updated config");

        assert!(outcome.installed);
        assert_eq!(
            outcome.backup_path,
            Some(config_path.with_extension("json.bak"))
        );
        assert!(content.contains("\"cancerbroker\""));
        assert!(
            outcome
                .backup_path
                .as_ref()
                .is_some_and(|path| path.exists())
        );
    }

    #[test]
    fn configure_opencode_removes_entry_when_uninstalling() {
        let dir = tempdir().expect("tempdir");
        let config_path = dir.path().join("opencode.json");
        fs::write(
            &config_path,
            serde_json::to_string(&json!({
                "mcp": {
                    "cancerbroker": cancerbroker_mcp_entry(),
                    "other": {
                        "type": "remote"
                    }
                }
            }))
            .expect("json"),
        )
        .expect("config file");

        let outcome = configure_opencode(&config_path, false).expect("uninstall outcome");
        let content = fs::read_to_string(&config_path).expect("updated config");

        assert!(!outcome.installed);
        assert!(!content.contains("\"cancerbroker\""));
        assert!(content.contains("\"other\""));
    }
}
