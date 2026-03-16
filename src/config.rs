use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::completion::CompletionSource;

#[cfg(unix)]
const DEFAULT_STORAGE_ALLOWLIST: &str = "~/.local/share/opencode/storage";
#[cfg(windows)]
const DEFAULT_STORAGE_ALLOWLIST: &str = "opencode\\storage";

const DEFAULT_COMMAND_MARKERS: [&str; 2] = ["opencode", "openagent"];

#[cfg(unix)]
const DEFAULT_IPC_SOCKET_PATH: &str = "/tmp/cancerbroker.sock";
#[cfg(windows)]
const DEFAULT_IPC_SOCKET_PATH: &str = r"\\.\pipe\cancerbroker";

#[cfg(unix)]
const DEFAULT_COMPLETION_SOCKET_PATH: &str = "/tmp/cancerbroker-completion.sock";
#[cfg(windows)]
const DEFAULT_COMPLETION_SOCKET_PATH: &str = r"\\.\pipe\cancerbroker-completion";

#[cfg(unix)]
const DEFAULT_COMPLETION_STATE_PATH: &str = "/tmp/cancerbroker-completion-state.json";
#[cfg(windows)]
const DEFAULT_COMPLETION_STATE_PATH: &str = "cancerbroker-completion-state.json";

#[cfg(unix)]
const DEFAULT_NOTIFICATION_SESSION_STATE_PATH: &str = "/tmp/cancerbroker-notify-session.json";
#[cfg(windows)]
const DEFAULT_NOTIFICATION_SESSION_STATE_PATH: &str = "cancerbroker-notify-session.json";

pub const DEFAULT_GUARDIAN_CONFIG_ENV: &str = "CANCERBROKER_CONFIG";

#[cfg(unix)]
const DEFAULT_GUARDIAN_CONFIG_RELATIVE_PATH: &str = ".config/cancerbroker/config.toml";
#[cfg(windows)]
const DEFAULT_GUARDIAN_CONFIG_RELATIVE_PATH: &str = "cancerbroker\\config.toml";

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
            allowlist: vec![PathBuf::from(DEFAULT_STORAGE_ALLOWLIST)],
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
            required_command_markers: DEFAULT_COMMAND_MARKERS
                .into_iter()
                .map(str::to_string)
                .collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeakDetectionPolicy {
    pub enabled: bool,
    pub required_consecutive_growth_samples: usize,
    pub minimum_rss_bytes: u64,
    pub minimum_growth_bytes_per_sample: u64,
}

impl Default for LeakDetectionPolicy {
    fn default() -> Self {
        Self {
            enabled: false,
            required_consecutive_growth_samples: 3,
            minimum_rss_bytes: 256 * 1024 * 1024,
            minimum_growth_bytes_per_sample: 32 * 1024 * 1024,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RustAnalyzerMemoryGuardPolicy {
    pub enabled: bool,
    pub max_rss_bytes: u64,
    pub required_consecutive_samples: usize,
    pub startup_grace_secs: u64,
    pub cooldown_secs: u64,
    pub same_uid_only: bool,
}

impl Default for RustAnalyzerMemoryGuardPolicy {
    fn default() -> Self {
        Self {
            enabled: true,
            max_rss_bytes: 500 * 1024 * 1024,
            required_consecutive_samples: 3,
            startup_grace_secs: 300,
            cooldown_secs: 1800,
            same_uid_only: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IpcConfig {
    pub enabled: bool,
    pub socket_path: PathBuf,
}

impl Default for IpcConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            socket_path: PathBuf::from(DEFAULT_IPC_SOCKET_PATH),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompletionCleanupPolicy {
    pub enabled_sources: Vec<CompletionSource>,
    pub dedupe_ttl_secs: u64,
    pub cleanup_retry_interval_secs: u64,
    pub reconciliation_interval_secs: u64,
    pub daemon_socket_path: PathBuf,
    pub state_path: PathBuf,
}

impl Default for CompletionCleanupPolicy {
    fn default() -> Self {
        Self {
            enabled_sources: vec![
                CompletionSource::Status,
                CompletionSource::Idle,
                CompletionSource::ToolPartCompleted,
                CompletionSource::Error,
                CompletionSource::Deleted,
                CompletionSource::Inferred,
            ],
            dedupe_ttl_secs: 600,
            cleanup_retry_interval_secs: 15,
            reconciliation_interval_secs: 60,
            daemon_socket_path: PathBuf::from(DEFAULT_COMPLETION_SOCKET_PATH),
            state_path: default_completion_state_path(),
        }
    }
}

#[cfg(unix)]
fn default_completion_state_path() -> PathBuf {
    PathBuf::from(DEFAULT_COMPLETION_STATE_PATH)
}

#[cfg(windows)]
fn default_completion_state_path() -> PathBuf {
    std::env::temp_dir().join(DEFAULT_COMPLETION_STATE_PATH)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotificationPolicy {
    pub session_state_path: PathBuf,
}

impl Default for NotificationPolicy {
    fn default() -> Self {
        Self {
            session_state_path: default_notification_session_state_path(),
        }
    }
}

#[cfg(unix)]
pub fn default_notification_session_state_path() -> PathBuf {
    PathBuf::from(DEFAULT_NOTIFICATION_SESSION_STATE_PATH)
}

#[cfg(windows)]
pub fn default_notification_session_state_path() -> PathBuf {
    std::env::temp_dir().join(DEFAULT_NOTIFICATION_SESSION_STATE_PATH)
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
    #[serde(default)]
    pub leak_detection: LeakDetectionPolicy,
    #[serde(default)]
    pub rust_analyzer_memory_guard: RustAnalyzerMemoryGuardPolicy,
    #[serde(default)]
    pub ipc: IpcConfig,
    #[serde(default)]
    pub completion: CompletionCleanupPolicy,
    #[serde(default)]
    pub notifications: NotificationPolicy,
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
            leak_detection: LeakDetectionPolicy::default(),
            rust_analyzer_memory_guard: RustAnalyzerMemoryGuardPolicy::default(),
            ipc: IpcConfig::default(),
            completion: CompletionCleanupPolicy::default(),
            notifications: NotificationPolicy::default(),
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

pub fn default_guardian_config_path(home: &Path) -> PathBuf {
    home.join(DEFAULT_GUARDIAN_CONFIG_RELATIVE_PATH)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use tempfile::tempdir;

    use super::{
        CompletionSource, DEFAULT_COMMAND_MARKERS, DEFAULT_COMPLETION_SOCKET_PATH,
        DEFAULT_GUARDIAN_CONFIG_RELATIVE_PATH, DEFAULT_IPC_SOCKET_PATH, DEFAULT_STORAGE_ALLOWLIST,
        GuardianConfig, LeakDetectionPolicy, Mode, RustAnalyzerMemoryGuardPolicy,
        default_completion_state_path, default_guardian_config_path,
        default_notification_session_state_path, load_config,
    };

    fn fixture_config_path(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("fixtures")
            .join("config")
            .join(name)
    }

    #[test]
    fn defaults_keep_opencode_paths_and_markers() {
        let config = GuardianConfig::default();

        assert_eq!(config.mode, Mode::Observe);
        assert_eq!(
            config.storage.allowlist,
            vec![PathBuf::from(DEFAULT_STORAGE_ALLOWLIST)]
        );
        assert_eq!(
            config.safety.required_command_markers,
            DEFAULT_COMMAND_MARKERS.map(str::to_string).to_vec()
        );
        assert_eq!(
            config.ipc.socket_path.to_string_lossy(),
            DEFAULT_IPC_SOCKET_PATH
        );
        assert_eq!(
            config.completion.daemon_socket_path.to_string_lossy(),
            DEFAULT_COMPLETION_SOCKET_PATH
        );
        assert_eq!(
            config.completion.state_path,
            default_completion_state_path()
        );
        assert_eq!(
            config.notifications.session_state_path,
            default_notification_session_state_path()
        );
        assert_eq!(config.leak_detection, LeakDetectionPolicy::default());
        assert_eq!(
            config.rust_analyzer_memory_guard,
            RustAnalyzerMemoryGuardPolicy::default()
        );
    }

    #[test]
    fn load_config_applies_defaults_to_partial_files() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("guardian.toml");
        fs::write(&path, "mode = \"enforce\"\n").expect("config file should be written");

        let config = load_config(&path).expect("partial config should load");

        assert_eq!(config.mode, Mode::Enforce);
        assert_eq!(config.sampling.sample_interval_secs, 5);
        assert_eq!(
            config.safety.required_command_markers,
            DEFAULT_COMMAND_MARKERS.map(str::to_string).to_vec()
        );
    }

    #[test]
    fn load_config_parses_completion_overrides() {
        let dir = tempdir().expect("tempdir");
        let custom_socket = dir.path().join("custom-completion.sock");
        let custom_state = dir.path().join("custom-completion.json");
        let path = dir.path().join("guardian.toml");
        fs::write(
            &path,
            format!(
                r#"
                [completion]
                dedupe_ttl_secs = 42
                cleanup_retry_interval_secs = 9
                reconciliation_interval_secs = 12
                daemon_socket_path = {socket}
                state_path = {state}
                enabled_sources = ["status", "tool_part_completed"]
            "#,
                socket = toml::Value::String(custom_socket.to_string_lossy().into_owned()),
                state = toml::Value::String(custom_state.to_string_lossy().into_owned()),
            ),
        )
        .expect("config file should be written");

        let config = load_config(&path).expect("completion config should load");

        assert_eq!(config.completion.dedupe_ttl_secs, 42);
        assert_eq!(config.completion.cleanup_retry_interval_secs, 9);
        assert_eq!(config.completion.reconciliation_interval_secs, 12);
        assert_eq!(config.completion.daemon_socket_path, custom_socket);
        assert_eq!(config.completion.state_path, custom_state);
        assert_eq!(
            config.completion.enabled_sources,
            vec![
                CompletionSource::Status,
                CompletionSource::ToolPartCompleted
            ]
        );
    }

    #[test]
    fn load_config_reports_missing_files() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("missing.toml");

        let error = load_config(&path).expect_err("missing config should fail");

        assert!(error.to_string().contains("config read error"));
        assert!(error.to_string().contains("missing.toml"));
    }

    #[test]
    fn default_guardian_config_path_uses_home_directory() {
        let home = PathBuf::from("/tmp/home");
        assert_eq!(
            default_guardian_config_path(home.as_path()),
            home.join(DEFAULT_GUARDIAN_CONFIG_RELATIVE_PATH)
        );
    }

    #[test]
    fn load_config_parses_leak_detection_overrides() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("guardian.toml");
        fs::write(
            &path,
            r#"
                [leak_detection]
                enabled = true
                required_consecutive_growth_samples = 4
                minimum_rss_bytes = 1234
                minimum_growth_bytes_per_sample = 5678
            "#,
        )
        .expect("config file should be written");

        let config = load_config(&path).expect("leak detection config should load");

        assert!(config.leak_detection.enabled);
        assert_eq!(config.leak_detection.required_consecutive_growth_samples, 4);
        assert_eq!(config.leak_detection.minimum_rss_bytes, 1234);
        assert_eq!(config.leak_detection.minimum_growth_bytes_per_sample, 5678);
    }

    #[test]
    fn load_config_parses_rust_analyzer_memory_guard_overrides() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("guardian.toml");
        fs::write(
            &path,
            r#"
                [rust_analyzer_memory_guard]
                enabled = true
                max_rss_bytes = 600000000
                required_consecutive_samples = 2
                startup_grace_secs = 60
                cooldown_secs = 120
                same_uid_only = false
            "#,
        )
        .expect("config file should be written");

        let config = load_config(&path).expect("rust-analyzer memory guard config should load");

        assert!(config.rust_analyzer_memory_guard.enabled);
        assert_eq!(config.rust_analyzer_memory_guard.max_rss_bytes, 600000000);
        assert_eq!(
            config
                .rust_analyzer_memory_guard
                .required_consecutive_samples,
            2
        );
        assert_eq!(config.rust_analyzer_memory_guard.startup_grace_secs, 60);
        assert_eq!(config.rust_analyzer_memory_guard.cooldown_secs, 120);
        assert!(!config.rust_analyzer_memory_guard.same_uid_only);
    }

    #[test]
    fn load_config_supports_ra_guard_minimal_fixture() {
        let path = fixture_config_path("rust-analyzer-guard-minimal.toml");

        let config = load_config(&path).expect("ra-guard minimal fixture should load");

        assert_eq!(config.mode, Mode::Observe);
        assert!(config.rust_analyzer_memory_guard.enabled);
        assert_eq!(
            config.rust_analyzer_memory_guard,
            RustAnalyzerMemoryGuardPolicy::default()
        );
    }

    #[test]
    fn load_config_keeps_existing_completion_fixture_compatible() {
        let path = fixture_config_path("completion-cleanup.toml");

        let config = load_config(&path).expect("completion fixture should load");

        assert_eq!(config.mode, Mode::Observe);
        assert_eq!(config.completion.dedupe_ttl_secs, 600);
        assert_eq!(config.completion.cleanup_retry_interval_secs, 15);
        assert_eq!(config.completion.reconciliation_interval_secs, 60);
        assert_eq!(
            config.completion.daemon_socket_path,
            PathBuf::from("/tmp/cancerbroker-completion.sock")
        );
        assert_eq!(
            config.completion.state_path,
            PathBuf::from("/tmp/cancerbroker-completion-state.json")
        );
    }
}
