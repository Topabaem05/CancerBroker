use std::env;
use std::fs;
use std::io::{self, BufRead, IsTerminal, Write};
use std::path::{Path, PathBuf};

use color_eyre::eyre::Result;
use serde_json::{Map, Value, json};
use thiserror::Error;
use toml::Table as TomlTable;

use crate::config::{
    ConfigError, DEFAULT_GUARDIAN_CONFIG_ENV, GuardianConfig, RustAnalyzerMemoryGuardPolicy,
    default_guardian_config_path, load_config,
};
use crate::setup_ui::{SetupWizardAnswers, SetupWizardDefaults, run_setup_wizard};

const OPENCODE_CONFIG_RELATIVE_PATH: &str = ".config/opencode/opencode.json";
const CANCERBROKER_MCP_KEY: &str = "cancerbroker";
const CANCERBROKER_MCP_TIMEOUT_MS: u64 = 30_000;
const GIB_BYTES: u64 = 1024 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SetupOptions {
    pub interactive: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetupOutcome {
    pub opencode_config_path: PathBuf,
    pub opencode_backup_path: Option<PathBuf>,
    pub guardian_config_path: Option<PathBuf>,
    pub guardian_backup_path: Option<PathBuf>,
    pub installed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RecommendedMemoryDefaults {
    detected_ram_gb: Option<u64>,
    memory_cap_gb: u64,
    required_consecutive_samples: usize,
    startup_grace_secs: u64,
    cooldown_secs: u64,
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
    #[error("guardian config parse error at {path}: {source}")]
    GuardianParse {
        path: String,
        source: toml::de::Error,
    },
    #[error("guardian config serialize error at {path}: {source}")]
    GuardianSerialize {
        path: String,
        source: toml::ser::Error,
    },
    #[error("guardian config root must be a TOML table")]
    InvalidGuardianRoot,
    #[error("guardian rust_analyzer_memory_guard section must be a TOML table")]
    InvalidGuardianSection,
    #[error("setup prompt I/O error: {source}")]
    PromptIo { source: std::io::Error },
    #[error("setup value for {field} is out of TOML integer range: {value}")]
    ValueOutOfRange { field: &'static str, value: u64 },
    #[error(transparent)]
    GuardianConfig(#[from] ConfigError),
}

fn home_dir() -> Option<PathBuf> {
    env::var_os("HOME").map(PathBuf::from)
}

fn opencode_config_path(home: &Path) -> PathBuf {
    home.join(OPENCODE_CONFIG_RELATIVE_PATH)
}

fn guardian_config_path(home: &Path) -> PathBuf {
    env::var_os(DEFAULT_GUARDIAN_CONFIG_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| default_guardian_config_path(home))
}

fn backup_path(config_path: &Path) -> PathBuf {
    match config_path
        .extension()
        .and_then(|extension| extension.to_str())
    {
        Some(extension) => config_path.with_extension(format!("{extension}.bak")),
        None => config_path.with_extension("bak"),
    }
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

fn read_guardian_config_document(config_path: &Path) -> Result<toml::Value, SetupError> {
    if !config_path.exists() {
        return Ok(toml::Value::Table(TomlTable::new()));
    }

    let content = fs::read_to_string(config_path).map_err(|source| SetupError::Read {
        path: config_path.display().to_string(),
        source,
    })?;

    toml::from_str(&content).map_err(|source| SetupError::GuardianParse {
        path: config_path.display().to_string(),
        source,
    })
}

fn write_guardian_config_document(
    config_path: &Path,
    root: &toml::Value,
) -> Result<(), SetupError> {
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent).map_err(|source| SetupError::Write {
            path: parent.display().to_string(),
            source,
        })?;
    }

    let content = toml::to_string_pretty(root).map_err(|source| SetupError::GuardianSerialize {
        path: config_path.display().to_string(),
        source,
    })?;
    fs::write(config_path, format!("{content}\n")).map_err(|source| SetupError::Write {
        path: config_path.display().to_string(),
        source,
    })?;
    Ok(())
}

fn ensure_guardian_root(root: &mut toml::Value) -> Result<&mut TomlTable, SetupError> {
    root.as_table_mut().ok_or(SetupError::InvalidGuardianRoot)
}

fn ensure_guardian_section(root: &mut toml::Value) -> Result<&mut TomlTable, SetupError> {
    let root = ensure_guardian_root(root)?;
    let section = root
        .entry("rust_analyzer_memory_guard".to_string())
        .or_insert_with(|| toml::Value::Table(TomlTable::new()));
    section
        .as_table_mut()
        .ok_or(SetupError::InvalidGuardianSection)
}

fn toml_integer(field: &'static str, value: u64) -> Result<toml::Value, SetupError> {
    let value = i64::try_from(value).map_err(|_| SetupError::ValueOutOfRange { field, value })?;
    Ok(toml::Value::Integer(value))
}

fn update_guardian_config(
    root: &mut toml::Value,
    answers: &SetupWizardAnswers,
    same_uid_only: bool,
) -> Result<(), SetupError> {
    let section = ensure_guardian_section(root)?;
    let max_rss_bytes = gib_to_bytes(answers.memory_cap_gb)?;
    section.insert("enabled".to_string(), toml::Value::Boolean(answers.enabled));
    section.insert(
        "max_rss_bytes".to_string(),
        toml_integer("max_rss_bytes", max_rss_bytes)?,
    );
    section.insert(
        "required_consecutive_samples".to_string(),
        toml_integer(
            "required_consecutive_samples",
            answers.required_consecutive_samples as u64,
        )?,
    );
    section.insert(
        "startup_grace_secs".to_string(),
        toml_integer("startup_grace_secs", answers.startup_grace_secs)?,
    );
    section.insert(
        "cooldown_secs".to_string(),
        toml_integer("cooldown_secs", answers.cooldown_secs)?,
    );
    section.insert(
        "same_uid_only".to_string(),
        toml::Value::Boolean(same_uid_only),
    );
    Ok(())
}

fn load_existing_guardian_settings(
    config_path: &Path,
) -> Result<Option<GuardianConfig>, SetupError> {
    if !config_path.exists() {
        return Ok(None);
    }

    load_config(config_path).map(Some).map_err(SetupError::from)
}

fn total_system_memory_bytes() -> Option<u64> {
    let mut system = sysinfo::System::new();
    system.refresh_memory();
    let total = system.total_memory();
    (total > 0).then_some(total)
}

fn bytes_to_display_gb(bytes: u64) -> u64 {
    if bytes == 0 {
        1
    } else {
        bytes.div_ceil(GIB_BYTES)
    }
}

fn gib_to_bytes(gb: u64) -> Result<u64, SetupError> {
    gb.checked_mul(GIB_BYTES)
        .ok_or(SetupError::ValueOutOfRange {
            field: "max_rss_bytes",
            value: gb,
        })
}

fn recommended_memory_defaults(total_memory_bytes: Option<u64>) -> RecommendedMemoryDefaults {
    let detected_ram_gb = total_memory_bytes.map(bytes_to_display_gb);
    match detected_ram_gb {
        Some(0..=8) => RecommendedMemoryDefaults {
            detected_ram_gb,
            memory_cap_gb: 1,
            required_consecutive_samples: 2,
            startup_grace_secs: 120,
            cooldown_secs: 600,
        },
        Some(9..=16) => RecommendedMemoryDefaults {
            detected_ram_gb,
            memory_cap_gb: 2,
            required_consecutive_samples: 2,
            startup_grace_secs: 180,
            cooldown_secs: 900,
        },
        Some(17..=32) => RecommendedMemoryDefaults {
            detected_ram_gb,
            memory_cap_gb: 4,
            required_consecutive_samples: 3,
            startup_grace_secs: 300,
            cooldown_secs: 1200,
        },
        Some(_) => RecommendedMemoryDefaults {
            detected_ram_gb,
            memory_cap_gb: 6,
            required_consecutive_samples: 3,
            startup_grace_secs: 300,
            cooldown_secs: 1800,
        },
        None => RecommendedMemoryDefaults {
            detected_ram_gb,
            memory_cap_gb: 1,
            required_consecutive_samples: 3,
            startup_grace_secs: 300,
            cooldown_secs: 1800,
        },
    }
}

fn build_wizard_defaults(existing: Option<&GuardianConfig>) -> SetupWizardDefaults {
    let recommended = recommended_memory_defaults(total_system_memory_bytes());
    let mut defaults = SetupWizardDefaults {
        detected_ram_gb: recommended.detected_ram_gb,
        enabled: true,
        memory_cap_gb: recommended.memory_cap_gb,
        required_consecutive_samples: recommended.required_consecutive_samples,
        startup_grace_secs: recommended.startup_grace_secs,
        cooldown_secs: recommended.cooldown_secs,
    };

    if let Some(config) = existing {
        let guard = &config.rust_analyzer_memory_guard;
        defaults.enabled = guard.enabled;
        defaults.memory_cap_gb = bytes_to_display_gb(guard.max_rss_bytes);
        defaults.required_consecutive_samples = guard.required_consecutive_samples;
        defaults.startup_grace_secs = guard.startup_grace_secs;
        defaults.cooldown_secs = guard.cooldown_secs;
    }

    defaults
}

fn default_same_uid_only(existing: Option<&GuardianConfig>) -> bool {
    existing
        .map(|config| config.rust_analyzer_memory_guard.same_uid_only)
        .unwrap_or_else(|| RustAnalyzerMemoryGuardPolicy::default().same_uid_only)
}

fn default_answers(defaults: &SetupWizardDefaults) -> SetupWizardAnswers {
    SetupWizardAnswers {
        enabled: defaults.enabled,
        memory_cap_gb: defaults.memory_cap_gb,
        required_consecutive_samples: defaults.required_consecutive_samples,
        startup_grace_secs: defaults.startup_grace_secs,
        cooldown_secs: defaults.cooldown_secs,
    }
}

fn install_with_io<R: BufRead, W: Write>(
    opencode_path: &Path,
    guardian_path: &Path,
    options: SetupOptions,
    reader: &mut R,
    writer: &mut W,
) -> Result<SetupOutcome, SetupError> {
    let existing_guardian = load_existing_guardian_settings(guardian_path)?;
    let defaults = build_wizard_defaults(existing_guardian.as_ref());
    let answers = if options.interactive {
        run_setup_wizard(reader, writer, &defaults)
            .map_err(|source| SetupError::PromptIo { source })?
    } else {
        default_answers(&defaults)
    };

    let mut opencode_root = read_opencode_config(opencode_path)?;
    let opencode_backup_path = write_backup_if_present(opencode_path)?;
    update_opencode_config(&mut opencode_root, true)?;
    write_opencode_config(opencode_path, &opencode_root)?;

    let mut guardian_root = read_guardian_config_document(guardian_path)?;
    let guardian_backup_path = write_backup_if_present(guardian_path)?;
    update_guardian_config(
        &mut guardian_root,
        &answers,
        default_same_uid_only(existing_guardian.as_ref()),
    )?;
    write_guardian_config_document(guardian_path, &guardian_root)?;

    Ok(SetupOutcome {
        opencode_config_path: opencode_path.to_path_buf(),
        opencode_backup_path,
        guardian_config_path: Some(guardian_path.to_path_buf()),
        guardian_backup_path,
        installed: true,
    })
}

fn uninstall_opencode(config_path: &Path) -> Result<SetupOutcome, SetupError> {
    if !config_path.exists() {
        return Ok(SetupOutcome {
            opencode_config_path: config_path.to_path_buf(),
            opencode_backup_path: None,
            guardian_config_path: None,
            guardian_backup_path: None,
            installed: false,
        });
    }

    let mut root = read_opencode_config(config_path)?;
    let opencode_backup_path = write_backup_if_present(config_path)?;
    update_opencode_config(&mut root, false)?;
    write_opencode_config(config_path, &root)?;

    Ok(SetupOutcome {
        opencode_config_path: config_path.to_path_buf(),
        opencode_backup_path,
        guardian_config_path: None,
        guardian_backup_path: None,
        installed: false,
    })
}

pub fn setup(options: SetupOptions) -> Result<SetupOutcome> {
    let home = home_dir().ok_or(SetupError::MissingHome)?;
    let mut stdin = io::stdin().lock();
    let mut stdout = io::stdout().lock();
    install_with_io(
        &opencode_config_path(&home),
        &guardian_config_path(&home),
        options,
        &mut stdin,
        &mut stdout,
    )
    .map_err(Into::into)
}

pub fn default_setup_options(non_interactive: bool) -> SetupOptions {
    SetupOptions {
        interactive: !non_interactive && io::stdin().is_terminal() && io::stdout().is_terminal(),
    }
}

pub fn uninstall() -> Result<SetupOutcome> {
    let home = home_dir().ok_or(SetupError::MissingHome)?;
    uninstall_opencode(&opencode_config_path(&home)).map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::Cursor;
    use std::path::PathBuf;

    use serde_json::json;
    use tempfile::tempdir;

    use super::{
        CANCERBROKER_MCP_KEY, GIB_BYTES, SetupOptions, TomlTable, backup_path,
        build_wizard_defaults, cancerbroker_mcp_entry, default_answers, default_same_uid_only,
        default_setup_options, gib_to_bytes, guardian_config_path, install_with_io,
        opencode_config_path, read_guardian_config_document, recommended_memory_defaults,
        uninstall_opencode, update_guardian_config, update_opencode_config,
    };
    use crate::config::GuardianConfig;
    use crate::setup_ui::SetupWizardAnswers;

    #[test]
    fn opencode_config_path_joins_home_directory() {
        assert_eq!(
            opencode_config_path(PathBuf::from("/tmp/home").as_path()),
            PathBuf::from("/tmp/home/.config/opencode/opencode.json")
        );
    }

    #[test]
    fn guardian_config_path_uses_default_home_path() {
        assert_eq!(
            guardian_config_path(PathBuf::from("/tmp/home").as_path()),
            PathBuf::from("/tmp/home/.config/cancerbroker/config.toml")
        );
    }

    #[test]
    fn backup_path_keeps_original_extension() {
        assert_eq!(
            backup_path(PathBuf::from("/tmp/opencode.json").as_path()),
            PathBuf::from("/tmp/opencode.json.bak")
        );
        assert_eq!(
            backup_path(PathBuf::from("/tmp/config.toml").as_path()),
            PathBuf::from("/tmp/config.toml.bak")
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
    fn update_opencode_config_preserves_unrelated_sections() {
        let mut root = json!({
            "mcp": {},
            "plugin": ["oh-my-opencode@latest"]
        });

        update_opencode_config(&mut root, true).expect("setup update");

        assert_eq!(root["plugin"][0], "oh-my-opencode@latest");
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

        update_opencode_config(&mut root, false).expect("setup update");

        assert!(root["mcp"].get(CANCERBROKER_MCP_KEY).is_none());
        assert_eq!(root["mcp"]["other"]["type"], "remote");
    }

    #[test]
    fn update_guardian_config_writes_guard_section() {
        let mut root = toml::Value::Table(TomlTable::new());
        let answers = SetupWizardAnswers {
            enabled: true,
            memory_cap_gb: 2,
            required_consecutive_samples: 2,
            startup_grace_secs: 180,
            cooldown_secs: 900,
        };

        update_guardian_config(&mut root, &answers, true).expect("guardian update");

        assert_eq!(
            root["rust_analyzer_memory_guard"]["max_rss_bytes"].as_integer(),
            Some((2_u64 * 1024 * 1024 * 1024) as i64)
        );
        assert_eq!(
            root["rust_analyzer_memory_guard"]["same_uid_only"].as_bool(),
            Some(true)
        );
    }

    #[test]
    fn recommended_memory_defaults_follow_ram_buckets() {
        assert_eq!(
            recommended_memory_defaults(Some(8 * GIB_BYTES)).memory_cap_gb,
            1
        );
        assert_eq!(
            recommended_memory_defaults(Some(16 * GIB_BYTES)).memory_cap_gb,
            2
        );
        assert_eq!(
            recommended_memory_defaults(Some(32 * GIB_BYTES)).memory_cap_gb,
            4
        );
        assert_eq!(
            recommended_memory_defaults(Some(64 * GIB_BYTES)).memory_cap_gb,
            6
        );
        assert_eq!(recommended_memory_defaults(None).memory_cap_gb, 1);
    }

    #[test]
    fn build_wizard_defaults_prefers_existing_guard_values() {
        let existing = GuardianConfig {
            rust_analyzer_memory_guard: crate::config::RustAnalyzerMemoryGuardPolicy {
                enabled: false,
                max_rss_bytes: 600_000_000,
                required_consecutive_samples: 4,
                startup_grace_secs: 90,
                cooldown_secs: 240,
                same_uid_only: false,
            },
            ..GuardianConfig::default()
        };

        let defaults = build_wizard_defaults(Some(&existing));

        assert!(!defaults.enabled);
        assert_eq!(defaults.memory_cap_gb, 1);
        assert_eq!(defaults.required_consecutive_samples, 4);
        assert_eq!(defaults.startup_grace_secs, 90);
        assert_eq!(defaults.cooldown_secs, 240);
    }

    #[test]
    fn install_with_io_writes_both_configs_non_interactively() {
        let dir = tempdir().expect("tempdir");
        let opencode_path = dir.path().join("opencode.json");
        let guardian_path = dir.path().join("guardian.toml");
        let mut input = Cursor::new(Vec::<u8>::new());
        let mut output = Vec::new();

        let expected_answers = default_answers(&build_wizard_defaults(None));
        let expected_bytes = gib_to_bytes(expected_answers.memory_cap_gb).expect("expected bytes");

        let outcome = install_with_io(
            &opencode_path,
            &guardian_path,
            SetupOptions { interactive: false },
            &mut input,
            &mut output,
        )
        .expect("setup outcome");

        let opencode_content = fs::read_to_string(&opencode_path).expect("updated opencode config");
        let guardian_content = fs::read_to_string(&guardian_path).expect("updated guardian config");

        assert!(outcome.installed);
        assert!(opencode_content.contains("\"cancerbroker\""));
        assert!(guardian_content.contains("[rust_analyzer_memory_guard]"));
        assert!(guardian_content.contains(&format!("max_rss_bytes = {expected_bytes}")));
        assert!(guardian_content.contains("same_uid_only = true"));
        assert!(output.is_empty());
    }

    #[test]
    fn install_with_io_creates_backups_and_preserves_existing_guardian_values() {
        let dir = tempdir().expect("tempdir");
        let opencode_path = dir.path().join("opencode.json");
        let guardian_path = dir.path().join("guardian.toml");
        fs::write(&opencode_path, "{\"mcp\":{}}\n").expect("opencode file");
        fs::write(
            &guardian_path,
            r#"
                mode = "observe"

                [rust_analyzer_memory_guard]
                enabled = true
                max_rss_bytes = 6442450944
                required_consecutive_samples = 3
                startup_grace_secs = 300
                cooldown_secs = 1800
                same_uid_only = false
            "#,
        )
        .expect("guardian file");
        let mut input = Cursor::new("no\n");
        let mut output = Vec::new();

        let outcome = install_with_io(
            &opencode_path,
            &guardian_path,
            SetupOptions { interactive: true },
            &mut input,
            &mut output,
        )
        .expect("setup outcome");
        let guardian_content = fs::read_to_string(&guardian_path).expect("guardian config");

        assert_eq!(
            outcome.opencode_backup_path,
            Some(opencode_path.with_extension("json.bak"))
        );
        assert_eq!(
            outcome.guardian_backup_path,
            Some(guardian_path.with_extension("toml.bak"))
        );
        assert!(guardian_content.contains("enabled = false"));
        assert!(guardian_content.contains("same_uid_only = false"));
    }

    #[test]
    fn uninstall_opencode_removes_entry_when_uninstalling() {
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

        let outcome = uninstall_opencode(&config_path).expect("uninstall outcome");
        let content = fs::read_to_string(&config_path).expect("updated config");

        assert!(!outcome.installed);
        assert!(outcome.guardian_config_path.is_none());
        assert!(!content.contains("\"cancerbroker\""));
        assert!(content.contains("\"other\""));
    }

    #[test]
    fn default_answers_match_wizard_defaults() {
        let defaults = build_wizard_defaults(None);
        let answers = default_answers(&defaults);

        assert_eq!(answers.enabled, defaults.enabled);
        assert_eq!(answers.memory_cap_gb, defaults.memory_cap_gb);
        assert_eq!(answers.cooldown_secs, defaults.cooldown_secs);
    }

    #[test]
    fn default_same_uid_only_uses_existing_or_runtime_default() {
        assert!(default_same_uid_only(None));

        let existing = GuardianConfig {
            rust_analyzer_memory_guard: crate::config::RustAnalyzerMemoryGuardPolicy {
                same_uid_only: false,
                ..crate::config::RustAnalyzerMemoryGuardPolicy::default()
            },
            ..GuardianConfig::default()
        };

        assert!(!default_same_uid_only(Some(&existing)));
    }

    #[test]
    fn gib_to_bytes_converts_whole_gigabytes() {
        assert_eq!(gib_to_bytes(2).expect("bytes"), 2 * GIB_BYTES);
    }

    #[test]
    fn default_setup_options_disables_interaction_outside_tty_when_requested() {
        let options = default_setup_options(true);
        assert!(!options.interactive);
    }

    #[test]
    fn read_guardian_config_document_returns_empty_table_for_missing_file() {
        let dir = tempdir().expect("tempdir");
        let missing = dir.path().join("missing.toml");

        let root = read_guardian_config_document(&missing).expect("empty root");

        assert!(root.as_table().is_some_and(|table| table.is_empty()));
    }
}
