use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use color_eyre::eyre::{Result, WrapErr};
use rmcp::handler::server::{router::tool::ToolRouter, wrapper::Parameters};
use rmcp::model::{Implementation, ServerCapabilities, ServerInfo};
use rmcp::transport::io::stdio;
use rmcp::{ServerHandler, ServiceExt, tool, tool_handler, tool_router};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::cli::default_signal_windows;
use crate::config::{GuardianConfig, load_config};
use crate::evidence::default_evidence_dir;
use crate::leak::{LeakCandidate, LeakDetector};
use crate::monitor::process::{ProcessInventory, ProcessSample};
use crate::monitor::resources::{ProcessResourceReport, collect_process_resources};
use crate::platform::current_effective_uid;
use crate::runtime::{RuntimeInput, RuntimeOutcome, run_once};
use crate::safety::OwnershipPolicy;

const DEFAULT_CONFIG_ENV: &str = "CANCERBROKER_CONFIG";
const DEFAULT_CONFIG_RELATIVE_PATH: &str = ".config/cancerbroker/config.toml";

#[derive(Debug, Clone)]
struct LoadedServerConfig {
    path: Option<PathBuf>,
    config: GuardianConfig,
}

#[derive(Debug, Clone)]
pub struct CancerBrokerMcp {
    config: GuardianConfig,
    config_path: Option<PathBuf>,
    leak_detector: Arc<Mutex<LeakDetector>>,
    tool_router: ToolRouter<Self>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
struct CleanupRequest {
    #[schemars(description = "Optional directory for cleanup evidence output.")]
    evidence_dir: Option<PathBuf>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
struct ListEvidenceRequest {
    #[schemars(description = "Optional directory to scan for evidence files.")]
    evidence_dir: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct StatusToolOutput {
    mode: String,
    config_source: String,
    config_path: Option<PathBuf>,
    required_command_markers: Vec<String>,
    allowlist: Vec<PathBuf>,
    default_evidence_dir: PathBuf,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
struct ScanProcessOutput {
    pid: u32,
    parent_pid: Option<u32>,
    pgid: Option<u32>,
    uid: Option<u32>,
    memory_bytes: u64,
    cpu_percent: f32,
    command: String,
    listening_ports: Vec<u16>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
struct ScanToolOutput {
    process_count: usize,
    total_memory_bytes: u64,
    processes: Vec<ScanProcessOutput>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct ScanResourcesProcessOutput {
    pid: u32,
    command: String,
    listening_ports: Vec<u16>,
    resources: ProcessResourceReport,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct ScanResourcesToolOutput {
    process_count: usize,
    processes: Vec<ScanResourcesProcessOutput>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct LeakCandidateOutput {
    pid: u32,
    pgid: Option<u32>,
    command: String,
    baseline_rss_bytes: u64,
    current_rss_bytes: u64,
    sample_count: usize,
    consecutive_growth_samples: usize,
    total_growth_bytes: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct ScanLeaksToolOutput {
    enabled: bool,
    required_consecutive_growth_samples: usize,
    minimum_rss_bytes: u64,
    minimum_growth_bytes_per_sample: u64,
    candidate_count: usize,
    candidates: Vec<LeakCandidateOutput>,
}

#[derive(Debug, Clone, Serialize)]
struct CleanupToolOutput {
    target_id: String,
    evidence_dir: PathBuf,
    outcome: RuntimeOutcome,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct EvidenceFileOutput {
    path: PathBuf,
    bytes: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct ListEvidenceToolOutput {
    evidence_dir: PathBuf,
    files: Vec<EvidenceFileOutput>,
}

fn home_dir() -> Option<PathBuf> {
    env::var_os("HOME").map(PathBuf::from)
}

fn default_config_path(home: Option<&Path>) -> Option<PathBuf> {
    home.map(|path| path.join(DEFAULT_CONFIG_RELATIVE_PATH))
}

fn resolve_server_config_path(
    cli_config: Option<PathBuf>,
    env_config: Option<PathBuf>,
    home: Option<&Path>,
) -> Option<PathBuf> {
    cli_config
        .or(env_config)
        .or_else(|| default_config_path(home).filter(|path| path.exists()))
}

fn load_server_config(cli_config: Option<PathBuf>) -> Result<LoadedServerConfig> {
    let env_config = env::var_os(DEFAULT_CONFIG_ENV).map(PathBuf::from);
    let resolved_path = resolve_server_config_path(cli_config, env_config, home_dir().as_deref());

    let config = match &resolved_path {
        Some(path) => load_config(path)
            .wrap_err_with(|| format!("config load failure at {}", path.display()))?,
        None => GuardianConfig::default(),
    };

    Ok(LoadedServerConfig {
        path: resolved_path,
        config,
    })
}

fn matches_command_markers(command: &str, required_command_markers: &[String]) -> bool {
    if required_command_markers.is_empty() {
        return true;
    }

    let command = command.to_lowercase();
    required_command_markers
        .iter()
        .any(|marker| command.contains(&marker.to_lowercase()))
}

fn serialize_tool_output<T: Serialize>(output: &T) -> Result<String, String> {
    serde_json::to_string_pretty(output).map_err(|error| error.to_string())
}

fn build_status_output(config: &GuardianConfig, config_path: Option<&Path>) -> StatusToolOutput {
    StatusToolOutput {
        mode: config.mode.as_str().to_string(),
        config_source: if config_path.is_some() {
            "file".to_string()
        } else {
            "default".to_string()
        },
        config_path: config_path.map(Path::to_path_buf),
        required_command_markers: config.safety.required_command_markers.clone(),
        allowlist: config.storage.allowlist.clone(),
        default_evidence_dir: default_evidence_dir(),
    }
}

fn build_ownership_policy(config: &GuardianConfig) -> OwnershipPolicy {
    OwnershipPolicy {
        expected_uid: current_effective_uid(),
        required_command_markers: config.safety.required_command_markers.clone(),
        same_uid_only: config.safety.same_uid_only,
    }
}

impl From<&ProcessSample> for ScanProcessOutput {
    fn from(sample: &ProcessSample) -> Self {
        Self {
            pid: sample.pid,
            parent_pid: sample.parent_pid,
            pgid: sample.pgid,
            uid: sample.uid,
            memory_bytes: sample.memory_bytes,
            cpu_percent: sample.cpu_percent,
            command: sample.command.clone(),
            listening_ports: sample.listening_ports.clone(),
        }
    }
}

fn build_scan_output(
    inventory: &ProcessInventory,
    required_command_markers: &[String],
) -> ScanToolOutput {
    let processes: Vec<_> = inventory
        .samples()
        .filter(|sample| matches_command_markers(&sample.command, required_command_markers))
        .map(ScanProcessOutput::from)
        .collect();

    let total_memory_bytes = processes.iter().fold(0_u64, |total, process| {
        total.saturating_add(process.memory_bytes)
    });

    ScanToolOutput {
        process_count: processes.len(),
        total_memory_bytes,
        processes,
    }
}

fn build_scan_resources_output(
    inventory: &ProcessInventory,
    required_command_markers: &[String],
) -> ScanResourcesToolOutput {
    let processes: Vec<_> = inventory
        .samples()
        .filter(|sample| matches_command_markers(&sample.command, required_command_markers))
        .map(|sample| ScanResourcesProcessOutput {
            pid: sample.pid,
            command: sample.command.clone(),
            listening_ports: sample.listening_ports.clone(),
            resources: collect_process_resources(sample.pid),
        })
        .collect();

    ScanResourcesToolOutput {
        process_count: processes.len(),
        processes,
    }
}

impl From<&LeakCandidate> for LeakCandidateOutput {
    fn from(candidate: &LeakCandidate) -> Self {
        Self {
            pid: candidate.identity.pid,
            pgid: candidate.identity.pgid,
            command: candidate.identity.command.clone(),
            baseline_rss_bytes: candidate.baseline_rss_bytes,
            current_rss_bytes: candidate.current_rss_bytes,
            sample_count: candidate.sample_count,
            consecutive_growth_samples: candidate.consecutive_growth_samples,
            total_growth_bytes: candidate.total_growth_bytes,
        }
    }
}

fn build_scan_leaks_output(
    detector: &Mutex<LeakDetector>,
    inventory: &ProcessInventory,
    config: &GuardianConfig,
) -> Result<ScanLeaksToolOutput, String> {
    let mut detector = detector
        .lock()
        .map_err(|_| "leak detector lock poisoned".to_string())?;
    let candidates = detector.observe_inventory(
        inventory,
        &config.leak_detection,
        &build_ownership_policy(config),
    );

    Ok(ScanLeaksToolOutput {
        enabled: config.leak_detection.enabled,
        required_consecutive_growth_samples: config
            .leak_detection
            .required_consecutive_growth_samples,
        minimum_rss_bytes: config.leak_detection.minimum_rss_bytes,
        minimum_growth_bytes_per_sample: config.leak_detection.minimum_growth_bytes_per_sample,
        candidate_count: candidates.len(),
        candidates: candidates.iter().map(LeakCandidateOutput::from).collect(),
    })
}

fn build_cleanup_output(config: &GuardianConfig, evidence_dir: PathBuf) -> CleanupToolOutput {
    let target_id = "mcp-target".to_string();
    let outcome = run_once(
        config,
        RuntimeInput {
            target_id: target_id.clone(),
            signal_windows: default_signal_windows(config),
            history: Vec::new(),
            now: SystemTime::now(),
            evidence_dir: evidence_dir.clone(),
        },
    );

    CleanupToolOutput {
        target_id,
        evidence_dir,
        outcome,
    }
}

fn list_evidence_files(evidence_dir: &Path) -> Result<Vec<EvidenceFileOutput>, String> {
    if !evidence_dir.exists() {
        return Ok(Vec::new());
    }

    let mut files = Vec::new();
    let entries = fs::read_dir(evidence_dir)
        .map_err(|error| format!("evidence read error at {}: {error}", evidence_dir.display()))?;

    for entry in entries {
        let entry = entry.map_err(|error| {
            format!(
                "evidence directory iteration error at {}: {error}",
                evidence_dir.display()
            )
        })?;
        let metadata = entry.metadata().map_err(|error| {
            format!(
                "evidence metadata read error at {}: {error}",
                entry.path().display()
            )
        })?;

        if metadata.is_file() {
            files.push(EvidenceFileOutput {
                path: entry.path(),
                bytes: metadata.len(),
            });
        }
    }

    files.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(files)
}

impl CancerBrokerMcp {
    pub fn new(config_path: Option<PathBuf>) -> Result<Self> {
        let loaded = load_server_config(config_path)?;

        Ok(Self {
            config: loaded.config,
            config_path: loaded.path,
            leak_detector: Arc::new(Mutex::new(LeakDetector::default())),
            tool_router: Self::tool_router(),
        })
    }
}

#[tool_router]
impl CancerBrokerMcp {
    #[tool(description = "Return the active CancerBroker mode and config summary.")]
    async fn status(&self) -> Result<String, String> {
        serialize_tool_output(&build_status_output(
            &self.config,
            self.config_path.as_deref(),
        ))
    }

    #[tool(description = "Scan live processes that match CancerBroker command markers.")]
    async fn scan(&self) -> Result<String, String> {
        let inventory = ProcessInventory::collect_live();
        serialize_tool_output(&build_scan_output(
            &inventory,
            &self.config.safety.required_command_markers,
        ))
    }

    #[tool(description = "Scan detailed open files and ports for matching live processes.")]
    async fn scan_resources(&self) -> Result<String, String> {
        let inventory = ProcessInventory::collect_live();
        serialize_tool_output(&build_scan_resources_output(
            &inventory,
            &self.config.safety.required_command_markers,
        ))
    }

    #[tool(description = "Scan live processes for repeated RSS growth candidates.")]
    async fn scan_leaks(&self) -> Result<String, String> {
        let inventory = ProcessInventory::collect_live();
        serialize_tool_output(&build_scan_leaks_output(
            &self.leak_detector,
            &inventory,
            &self.config,
        )?)
    }

    #[tool(description = "Run a single CancerBroker cleanup cycle.")]
    async fn cleanup(
        &self,
        Parameters(request): Parameters<CleanupRequest>,
    ) -> Result<String, String> {
        let evidence_dir = request.evidence_dir.unwrap_or_else(default_evidence_dir);
        serialize_tool_output(&build_cleanup_output(&self.config, evidence_dir))
    }

    #[tool(description = "List CancerBroker evidence files.")]
    async fn list_evidence(
        &self,
        Parameters(request): Parameters<ListEvidenceRequest>,
    ) -> Result<String, String> {
        let evidence_dir = request.evidence_dir.unwrap_or_else(default_evidence_dir);
        serialize_tool_output(&ListEvidenceToolOutput {
            evidence_dir: evidence_dir.clone(),
            files: list_evidence_files(&evidence_dir)?,
        })
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for CancerBrokerMcp {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            server_info: Implementation {
                name: "cancerbroker".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
                ..Implementation::default()
            },
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            instructions: Some(
                "CancerBroker exposes cleanup status, scan, cleanup, and evidence listing tools."
                    .to_string(),
            ),
            ..ServerInfo::default()
        }
    }
}

pub async fn run_mcp_server(config_path: Option<PathBuf>) -> Result<()> {
    let server = CancerBrokerMcp::new(config_path)?;
    let running = server
        .serve(stdio())
        .await
        .wrap_err("mcp stdio server start failure")?;
    let _ = running
        .waiting()
        .await
        .wrap_err("mcp stdio server wait failure")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::sync::Mutex;

    use rmcp::ServiceExt;
    use serde_json::{Value, json};
    use tempfile::tempdir;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    use super::{
        CancerBrokerMcp, build_cleanup_output, build_scan_leaks_output, build_scan_output,
        build_scan_resources_output, default_config_path, list_evidence_files,
        matches_command_markers, resolve_server_config_path,
    };
    use crate::config::{GuardianConfig, LeakDetectionPolicy};
    use crate::leak::LeakDetector;
    use crate::monitor::process::{ProcessInventory, ProcessSample};
    use crate::platform::current_effective_uid;

    fn sample_inventory() -> ProcessInventory {
        ProcessInventory::from_samples([
            ProcessSample {
                pid: 10,
                parent_pid: Some(1),
                pgid: Some(10),
                start_time_secs: 100,
                uid: Some(current_effective_uid()),
                memory_bytes: 128,
                cpu_percent: 0.5,
                command: "opencode ses_alpha worker".to_string(),
                listening_ports: vec![3000],
            },
            ProcessSample {
                pid: 20,
                parent_pid: Some(1),
                pgid: Some(20),
                start_time_secs: 200,
                uid: Some(current_effective_uid()),
                memory_bytes: 512,
                cpu_percent: 1.0,
                command: "python helper.py".to_string(),
                listening_ports: vec![],
            },
        ])
    }

    fn leak_inventory(memory_bytes: u64) -> ProcessInventory {
        ProcessInventory::from_samples([ProcessSample {
            pid: 10,
            parent_pid: Some(1),
            pgid: Some(10),
            start_time_secs: 100,
            uid: Some(current_effective_uid()),
            memory_bytes,
            cpu_percent: 0.5,
            command: "opencode ses_alpha worker".to_string(),
            listening_ports: vec![],
        }])
    }

    async fn write_transport_message(
        stream: &mut tokio::io::DuplexStream,
        message: Value,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut payload = serde_json::to_vec(&message)?;
        payload.push(b'\n');
        stream.write_all(&payload).await?;
        stream.flush().await?;
        Ok(())
    }

    async fn read_transport_message(
        stream: &mut tokio::io::DuplexStream,
    ) -> Result<Value, Box<dyn std::error::Error>> {
        let mut payload = Vec::new();

        loop {
            let byte = stream.read_u8().await?;
            if byte == b'\n' {
                break;
            }
            payload.push(byte);
        }

        Ok(serde_json::from_slice(&payload)?)
    }

    #[test]
    fn default_config_path_uses_home_directory() {
        assert_eq!(
            default_config_path(Some(PathBuf::from("/tmp/home").as_path())),
            Some(PathBuf::from("/tmp/home/.config/cancerbroker/config.toml"))
        );
    }

    #[test]
    fn resolve_server_config_path_prioritizes_cli_env_then_home_file() {
        let cli = Some(PathBuf::from("/tmp/cli.toml"));
        let env = Some(PathBuf::from("/tmp/env.toml"));
        let home = tempdir().expect("tempdir");
        let home_config = home.path().join(".config/cancerbroker/config.toml");
        fs::create_dir_all(home_config.parent().expect("config parent")).expect("config dir");
        fs::write(&home_config, "mode = \"observe\"\n").expect("config file");

        assert_eq!(
            resolve_server_config_path(cli.clone(), env.clone(), Some(home.path())),
            cli
        );
        assert_eq!(
            resolve_server_config_path(None, env.clone(), Some(home.path())),
            env
        );
        assert_eq!(
            resolve_server_config_path(None, None, Some(home.path())),
            Some(home_config)
        );
    }

    #[test]
    fn matches_command_markers_allows_empty_policy_and_detects_matches() {
        assert!(matches_command_markers("python helper.py", &[]));
        assert!(matches_command_markers(
            "OpenCode ses_alpha worker",
            &["opencode".to_string()]
        ));
        assert!(!matches_command_markers(
            "python helper.py",
            &["opencode".to_string()]
        ));
    }

    #[test]
    fn build_scan_output_filters_non_matching_processes() {
        let output = build_scan_output(&sample_inventory(), &["opencode".to_string()]);

        assert_eq!(output.process_count, 1);
        assert_eq!(output.total_memory_bytes, 128);
        assert_eq!(output.processes[0].pid, 10);
        assert_eq!(output.processes[0].listening_ports, vec![3000]);
    }

    #[test]
    fn build_cleanup_output_runs_single_cycle() {
        let dir = tempdir().expect("tempdir");
        let output = build_cleanup_output(&GuardianConfig::default(), dir.path().to_path_buf());

        assert_eq!(output.target_id, "mcp-target");
        assert_eq!(output.evidence_dir, dir.path());
        assert_eq!(
            output.outcome.proposed_action.as_deref(),
            Some("warn_throttle")
        );
    }

    #[test]
    fn build_scan_resources_output_collects_matching_process_reports() {
        let inventory = ProcessInventory::from_samples([ProcessSample {
            pid: std::process::id(),
            parent_pid: Some(1),
            pgid: Some(10),
            start_time_secs: 100,
            uid: Some(current_effective_uid()),
            memory_bytes: 128,
            cpu_percent: 0.5,
            command: "opencode ses_alpha worker".to_string(),
            listening_ports: vec![3000],
        }]);

        let output = build_scan_resources_output(&inventory, &["opencode".to_string()]);

        assert_eq!(output.process_count, 1);
        assert_eq!(output.processes[0].pid, std::process::id());
        assert_eq!(output.processes[0].listening_ports, vec![3000]);
    }

    #[test]
    fn build_scan_leaks_output_reports_candidates_after_repeated_growth() {
        let detector = Mutex::new(LeakDetector::default());
        let config = GuardianConfig {
            leak_detection: LeakDetectionPolicy {
                enabled: true,
                required_consecutive_growth_samples: 2,
                minimum_rss_bytes: 100,
                minimum_growth_bytes_per_sample: 20,
            },
            ..GuardianConfig::default()
        };

        assert_eq!(
            build_scan_leaks_output(&detector, &leak_inventory(100), &config)
                .expect("first output")
                .candidate_count,
            0
        );
        assert_eq!(
            build_scan_leaks_output(&detector, &leak_inventory(130), &config)
                .expect("second output")
                .candidate_count,
            0
        );

        let output = build_scan_leaks_output(&detector, &leak_inventory(160), &config)
            .expect("third output");

        assert!(output.enabled);
        assert_eq!(output.candidate_count, 1);
        assert_eq!(output.candidates[0].pid, 10);
        assert_eq!(output.candidates[0].total_growth_bytes, 60);
    }

    #[test]
    fn list_evidence_files_returns_sorted_files_only() {
        let dir = tempdir().expect("tempdir");
        let nested = dir.path().join("nested");
        fs::create_dir_all(&nested).expect("nested dir");
        fs::write(dir.path().join("b.json"), "{}\n").expect("b file");
        fs::write(dir.path().join("a.json"), "{}\n").expect("a file");

        let files = list_evidence_files(dir.path()).expect("evidence files");

        assert_eq!(files.len(), 2);
        assert_eq!(files[0].path, dir.path().join("a.json"));
        assert_eq!(files[1].path, dir.path().join("b.json"));
    }

    #[tokio::test]
    async fn status_tool_reports_default_config_without_file() {
        let server = CancerBrokerMcp::new(None).expect("server");
        let output = server.status().await.expect("status output");

        assert!(output.contains("\"config_source\": \"default\""));
        assert!(output.contains("\"mode\": \"observe\""));
    }

    #[tokio::test]
    async fn mcp_server_lists_and_calls_tools_over_transport() {
        let (mut client_transport, server_transport) = tokio::io::duplex(16 * 1024);
        let server = CancerBrokerMcp::new(None).expect("server");

        let server_task = tokio::spawn(async move {
            let running = server.serve(server_transport).await.expect("server start");
            let _ = running.waiting().await.expect("server wait");
        });

        write_transport_message(
            &mut client_transport,
            json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {
                    "protocolVersion": "2025-03-26",
                    "capabilities": {},
                    "clientInfo": {
                        "name": "cancerbroker-test-client",
                        "version": "0.1.0"
                    }
                }
            }),
        )
        .await
        .expect("initialize request");

        let initialize_response = read_transport_message(&mut client_transport)
            .await
            .expect("initialize response");
        assert_eq!(
            initialize_response["result"]["serverInfo"]["name"],
            "cancerbroker"
        );

        write_transport_message(
            &mut client_transport,
            json!({
                "jsonrpc": "2.0",
                "method": "notifications/initialized"
            }),
        )
        .await
        .expect("initialized notification");

        write_transport_message(
            &mut client_transport,
            json!({
                "jsonrpc": "2.0",
                "id": 2,
                "method": "tools/list",
                "params": {}
            }),
        )
        .await
        .expect("list tools request");

        let list_tools_response = read_transport_message(&mut client_transport)
            .await
            .expect("list tools response");
        let tool_names: Vec<_> = list_tools_response["result"]["tools"]
            .as_array()
            .expect("tool list")
            .iter()
            .filter_map(|tool| tool["name"].as_str())
            .collect();
        assert!(tool_names.contains(&"status"));
        assert!(tool_names.contains(&"scan"));
        assert!(tool_names.contains(&"scan_resources"));
        assert!(tool_names.contains(&"scan_leaks"));
        assert!(tool_names.contains(&"cleanup"));
        assert!(tool_names.contains(&"list_evidence"));

        write_transport_message(
            &mut client_transport,
            json!({
                "jsonrpc": "2.0",
                "id": 3,
                "method": "tools/call",
                "params": {
                    "name": "status",
                    "arguments": {}
                }
            }),
        )
        .await
        .expect("call tool request");

        let call_tool_response = read_transport_message(&mut client_transport)
            .await
            .expect("call tool response");
        let text = call_tool_response["result"]["content"]
            .as_array()
            .and_then(|content| content.first())
            .and_then(|content| content["text"].as_str())
            .expect("tool text response");
        assert!(text.contains("\"mode\": \"observe\""));

        drop(client_transport);
        server_task.await.expect("server task");
    }
}
