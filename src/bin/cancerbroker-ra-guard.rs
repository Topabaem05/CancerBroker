use std::collections::{BTreeMap, BTreeSet};
use std::ffi::{OsStr, OsString};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
#[cfg(unix)]
use std::thread;
use std::time::{Duration, SystemTime};

#[derive(Debug, Clone, PartialEq, Eq)]
struct MemoryGuardOutput {
    rust_analyzer_memory_candidates: usize,
    rust_analyzer_memory_remediations: usize,
}

impl Default for MemoryGuardOutput {
    fn default() -> Self {
        Self {
            rust_analyzer_memory_candidates: 0,
            rust_analyzer_memory_remediations: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
enum Mode {
    #[default]
    Observe,
    Enforce,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RustAnalyzerMemoryGuardPolicy {
    enabled: bool,
    max_rss_bytes: u64,
    required_consecutive_samples: usize,
    startup_grace_secs: u64,
    cooldown_secs: u64,
    same_uid_only: bool,
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct CompletionCleanupPolicy {
    cleanup_retry_interval_secs: u64,
}

impl Default for CompletionCleanupPolicy {
    fn default() -> Self {
        Self {
            cleanup_retry_interval_secs: 15,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct RaGuardConfig {
    mode: Mode,
    rust_analyzer_memory_guard: RustAnalyzerMemoryGuardPolicy,
    completion: CompletionCleanupPolicy,
}

#[derive(Debug)]
enum ConfigLoadError {
    Read {
        path: String,
        source: std::io::Error,
    },
    Parse {
        path: String,
        source: String,
    },
}

impl fmt::Display for ConfigLoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read { path, source } => {
                write!(f, "config read error at {path}: {source}")
            }
            Self::Parse { path, source } => {
                write!(f, "config parse error at {path}: {source}")
            }
        }
    }
}

fn load_ra_guard_config(path: &Path) -> Result<RaGuardConfig, ConfigLoadError> {
    let content = fs::read_to_string(path).map_err(|source| ConfigLoadError::Read {
        path: path.display().to_string(),
        source,
    })?;

    parse_ra_guard_config(&content).map_err(|source| ConfigLoadError::Parse {
        path: path.display().to_string(),
        source,
    })
}

fn parse_ra_guard_config(content: &str) -> Result<RaGuardConfig, String> {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Section {
        Root,
        RustAnalyzerMemoryGuard,
        Completion,
        Other,
    }

    let mut config = RaGuardConfig::default();
    let mut section = Section::Root;

    for (index, raw_line) in content.lines().enumerate() {
        let line_number = index + 1;
        let Some(line) = strip_toml_comment(raw_line) else {
            continue;
        };
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        if line.starts_with('[') {
            if !line.ends_with(']') {
                return Err(format!("line {line_number}: malformed section header"));
            }
            let header = line[1..line.len() - 1].trim();
            section = match header {
                "rust_analyzer_memory_guard" => Section::RustAnalyzerMemoryGuard,
                "completion" => Section::Completion,
                _ => Section::Other,
            };
            continue;
        }

        let Some((raw_key, raw_value)) = line.split_once('=') else {
            return Err(format!("line {line_number}: expected key = value"));
        };

        let key = raw_key.trim();
        let value = raw_value.trim();

        match (section, key) {
            (Section::Root, "mode") => {
                config.mode =
                    parse_mode(value).map_err(|err| format!("line {line_number}: {err}"))?
            }
            (Section::RustAnalyzerMemoryGuard, "enabled") => {
                config.rust_analyzer_memory_guard.enabled =
                    parse_bool(value).map_err(|err| format!("line {line_number}: {err}"))?
            }
            (Section::RustAnalyzerMemoryGuard, "max_rss_bytes") => {
                config.rust_analyzer_memory_guard.max_rss_bytes =
                    parse_u64(value).map_err(|err| format!("line {line_number}: {err}"))?
            }
            (Section::RustAnalyzerMemoryGuard, "required_consecutive_samples") => {
                config
                    .rust_analyzer_memory_guard
                    .required_consecutive_samples =
                    parse_usize(value).map_err(|err| format!("line {line_number}: {err}"))?
            }
            (Section::RustAnalyzerMemoryGuard, "startup_grace_secs") => {
                config.rust_analyzer_memory_guard.startup_grace_secs =
                    parse_u64(value).map_err(|err| format!("line {line_number}: {err}"))?
            }
            (Section::RustAnalyzerMemoryGuard, "cooldown_secs") => {
                config.rust_analyzer_memory_guard.cooldown_secs =
                    parse_u64(value).map_err(|err| format!("line {line_number}: {err}"))?
            }
            (Section::RustAnalyzerMemoryGuard, "same_uid_only") => {
                config.rust_analyzer_memory_guard.same_uid_only =
                    parse_bool(value).map_err(|err| format!("line {line_number}: {err}"))?
            }
            (Section::Completion, "cleanup_retry_interval_secs") => {
                config.completion.cleanup_retry_interval_secs =
                    parse_u64(value).map_err(|err| format!("line {line_number}: {err}"))?
            }
            _ => {}
        }
    }

    Ok(config)
}

fn strip_toml_comment(raw: &str) -> Option<&str> {
    let mut in_single_quotes = false;
    let mut in_double_quotes = false;
    let mut escaped = false;

    for (index, ch) in raw.char_indices() {
        if in_double_quotes {
            if escaped {
                escaped = false;
                continue;
            }
            match ch {
                '\\' => escaped = true,
                '"' => in_double_quotes = false,
                _ => {}
            }
            continue;
        }

        if in_single_quotes {
            if ch == '\'' {
                in_single_quotes = false;
            }
            continue;
        }

        match ch {
            '#' => return Some(&raw[..index]),
            '"' => in_double_quotes = true,
            '\'' => in_single_quotes = true,
            _ => {}
        }
    }

    Some(raw)
}

fn parse_mode(value: &str) -> Result<Mode, String> {
    match parse_string_token(value)?.to_ascii_lowercase().as_str() {
        "observe" => Ok(Mode::Observe),
        "enforce" => Ok(Mode::Enforce),
        other => Err(format!("unsupported mode '{other}'")),
    }
}

fn parse_string_token(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    if trimmed.len() >= 2
        && ((trimmed.starts_with('"') && trimmed.ends_with('"'))
            || (trimmed.starts_with('\'') && trimmed.ends_with('\'')))
    {
        return Ok(trimmed[1..trimmed.len() - 1].to_string());
    }
    if trimmed.is_empty() {
        return Err("expected non-empty string token".to_string());
    }
    Ok(trimmed.to_string())
}

fn parse_bool(value: &str) -> Result<bool, String> {
    match value.trim() {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(format!("invalid boolean '{value}'")),
    }
}

fn parse_u64(value: &str) -> Result<u64, String> {
    let normalized = value.trim().replace('_', "");
    normalized
        .parse::<u64>()
        .map_err(|_| format!("invalid u64 '{value}'"))
}

fn parse_usize(value: &str) -> Result<usize, String> {
    let normalized = value.trim().replace('_', "");
    normalized
        .parse::<usize>()
        .map_err(|_| format!("invalid usize '{value}'"))
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct GuardKey {
    pid: u32,
    start_time_secs: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProcessIdentity {
    pid: u32,
    parent_pid: Option<u32>,
    pgid: Option<u32>,
    start_time_secs: u64,
    uid: Option<u32>,
    command: String,
}

#[derive(Debug, Clone)]
struct GuardHistoryEntry {
    identity: ProcessIdentity,
    current_rss_bytes: u64,
    consecutive_over_limit_samples: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MemoryGuardCandidate {
    identity: ProcessIdentity,
    current_rss_bytes: u64,
    consecutive_over_limit_samples: usize,
}

#[derive(Debug, Clone, Default)]
struct RustAnalyzerMemoryGuard {
    histories: BTreeMap<GuardKey, GuardHistoryEntry>,
    last_remediation_unix_secs: Option<u64>,
}

#[derive(Debug, Clone)]
struct OwnershipPolicy {
    expected_uid: u32,
    required_command_markers: Vec<String>,
    same_uid_only: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum SafetyDecision {
    Allowed,
    Rejected(&'static str),
}

#[derive(Debug, Clone)]
struct ProcessSample {
    pid: u32,
    parent_pid: Option<u32>,
    pgid: Option<u32>,
    start_time_secs: u64,
    uid: Option<u32>,
    memory_bytes: u64,
    command: String,
}

#[derive(Debug, Clone, Default)]
struct ProcessInventory {
    processes: BTreeMap<u32, ProcessSample>,
}

impl ProcessInventory {
    fn from_samples(samples: impl IntoIterator<Item = ProcessSample>) -> Self {
        let mut processes = BTreeMap::new();
        for sample in samples {
            processes.insert(sample.pid, sample);
        }
        Self { processes }
    }

    fn samples(&self) -> impl Iterator<Item = &ProcessSample> {
        self.processes.values()
    }

    fn collect_live_for_rust_analyzer_guard() -> Self {
        #[cfg(target_os = "macos")]
        {
            return collect_live_rust_analyzer_macos().unwrap_or_default();
        }

        #[cfg(not(target_os = "macos"))]
        {
            collect_live_rust_analyzer_sysinfo()
        }
    }
}

fn render_output(output: &MemoryGuardOutput, json: bool) -> String {
    if json {
        format!(
            "{{\"rust_analyzer_memory_candidates\":{},\"rust_analyzer_memory_remediations\":{}}}",
            output.rust_analyzer_memory_candidates, output.rust_analyzer_memory_remediations
        )
    } else {
        format!(
            "rust_analyzer_memory_candidates={} rust_analyzer_memory_remediations={}",
            output.rust_analyzer_memory_candidates, output.rust_analyzer_memory_remediations
        )
    }
}

fn parse_args(args: impl IntoIterator<Item = OsString>) -> Result<(PathBuf, bool), String> {
    let mut args = args.into_iter();
    args.next();

    let first = args
        .next()
        .ok_or_else(|| "usage: cancerbroker-ra-guard --config <path> [--json]".to_string())?;
    if first != OsStr::new("--config") {
        return Err("expected --config as first argument".to_string());
    }

    let config_path = PathBuf::from(
        args.next()
            .ok_or_else(|| "missing value for --config".to_string())?,
    );

    let json = match args.next() {
        None => false,
        Some(flag) if flag == OsStr::new("--json") => true,
        Some(_) => return Err("only --json is supported after --config <path>".to_string()),
    };

    if args.next().is_some() {
        return Err("unexpected extra arguments".to_string());
    }

    Ok((config_path, json))
}

fn unix_timestamp_secs(now: SystemTime) -> u64 {
    now.duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn current_effective_uid() -> u32 {
    #[cfg(unix)]
    {
        nix::unistd::geteuid().as_raw()
    }

    #[cfg(not(unix))]
    {
        0
    }
}

#[cfg(not(target_os = "macos"))]
#[cfg(unix)]
fn process_group_id(pid: u32) -> Option<u32> {
    use nix::unistd::{getpgid, Pid};

    getpgid(Some(Pid::from_raw(pid as i32)))
        .ok()
        .map(|pgid| pgid.as_raw() as u32)
}

#[cfg(not(target_os = "macos"))]
#[cfg(not(unix))]
fn process_group_id(_pid: u32) -> Option<u32> {
    None
}

fn command_contains_marker(command: &str, required_command_markers: &[String]) -> bool {
    if required_command_markers.is_empty() {
        return true;
    }

    let command = command.to_lowercase();
    required_command_markers
        .iter()
        .any(|marker| command.contains(&marker.to_lowercase()))
}

fn validate_process_identity(
    identity: &ProcessIdentity,
    policy: &OwnershipPolicy,
) -> SafetyDecision {
    if policy.same_uid_only && identity.uid != Some(policy.expected_uid) {
        return SafetyDecision::Rejected("uid_mismatch");
    }

    if !command_contains_marker(&identity.command, &policy.required_command_markers) {
        return SafetyDecision::Rejected("command_marker_mismatch");
    }

    SafetyDecision::Allowed
}

fn command_contains_rust_analyzer(command: &str) -> bool {
    command.to_ascii_lowercase().contains("rust-analyzer")
}

fn sample_is_past_startup_grace(
    sample: &ProcessSample,
    policy: &RustAnalyzerMemoryGuardPolicy,
    now_unix_secs: u64,
) -> bool {
    now_unix_secs.saturating_sub(sample.start_time_secs) >= policy.startup_grace_secs
}

fn build_guard_key(sample: &ProcessSample) -> GuardKey {
    GuardKey {
        pid: sample.pid,
        start_time_secs: sample.start_time_secs,
    }
}

fn build_process_identity(sample: &ProcessSample) -> ProcessIdentity {
    ProcessIdentity {
        pid: sample.pid,
        parent_pid: sample.parent_pid,
        pgid: sample.pgid,
        start_time_secs: sample.start_time_secs,
        uid: sample.uid,
        command: sample.command.clone(),
    }
}

fn build_candidate(entry: &GuardHistoryEntry) -> MemoryGuardCandidate {
    MemoryGuardCandidate {
        identity: entry.identity.clone(),
        current_rss_bytes: entry.current_rss_bytes,
        consecutive_over_limit_samples: entry.consecutive_over_limit_samples,
    }
}

fn sample_matches_guard(
    sample: &ProcessSample,
    policy: &RustAnalyzerMemoryGuardPolicy,
    ownership_policy: &OwnershipPolicy,
    now_unix_secs: u64,
) -> Option<ProcessIdentity> {
    if !policy.enabled || !command_contains_rust_analyzer(&sample.command) {
        return None;
    }

    if !sample_is_past_startup_grace(sample, policy, now_unix_secs) {
        return None;
    }

    let identity = build_process_identity(sample);
    match validate_process_identity(&identity, ownership_policy) {
        SafetyDecision::Allowed => Some(identity),
        SafetyDecision::Rejected(_) => None,
    }
}

impl RustAnalyzerMemoryGuard {
    fn observe_inventory(
        &mut self,
        inventory: &ProcessInventory,
        policy: &RustAnalyzerMemoryGuardPolicy,
        ownership_policy: &OwnershipPolicy,
        now: SystemTime,
    ) -> Vec<MemoryGuardCandidate> {
        if !policy.enabled {
            self.histories.clear();
            self.last_remediation_unix_secs = None;
            return Vec::new();
        }

        let now_unix_secs = unix_timestamp_secs(now);
        let cooldown_active = self
            .last_remediation_unix_secs
            .is_some_and(|last| now_unix_secs.saturating_sub(last) < policy.cooldown_secs);

        let mut seen = BTreeSet::new();
        let mut candidates = Vec::new();

        for sample in inventory.samples() {
            let Some(identity) =
                sample_matches_guard(sample, policy, ownership_policy, now_unix_secs)
            else {
                continue;
            };

            let key = build_guard_key(sample);
            seen.insert(key.clone());

            let entry = self
                .histories
                .entry(key)
                .or_insert_with(|| GuardHistoryEntry {
                    identity: identity.clone(),
                    current_rss_bytes: sample.memory_bytes,
                    consecutive_over_limit_samples: 0,
                });

            entry.identity = identity;
            entry.current_rss_bytes = sample.memory_bytes;

            if sample.memory_bytes >= policy.max_rss_bytes {
                entry.consecutive_over_limit_samples += 1;
            } else {
                entry.consecutive_over_limit_samples = 0;
            }

            if !cooldown_active
                && entry.consecutive_over_limit_samples >= policy.required_consecutive_samples
            {
                candidates.push(build_candidate(entry));
            }
        }

        self.histories.retain(|key, _| seen.contains(key));
        candidates
    }

    fn record_remediation(&mut self, now: SystemTime) {
        self.last_remediation_unix_secs = Some(unix_timestamp_secs(now));
        self.histories.clear();
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ProcessRemediationOutcome {
    Rejected,
    AlreadyExited,
    TerminatedGracefully,
    TerminatedForced,
}

fn remediation_succeeded(outcome: &ProcessRemediationOutcome) -> bool {
    matches!(
        outcome,
        ProcessRemediationOutcome::TerminatedGracefully
            | ProcessRemediationOutcome::TerminatedForced
    )
}

#[cfg(unix)]
fn is_alive_unix(pid: nix::unistd::Pid) -> bool {
    use nix::errno::Errno;
    use nix::sys::signal::kill;

    match kill(pid, None) {
        Ok(()) => true,
        Err(Errno::ESRCH) => false,
        Err(_) => true,
    }
}

#[cfg(unix)]
fn wait_for_exit(pid: nix::unistd::Pid, timeout: Duration) -> bool {
    let start = std::time::Instant::now();
    while start.elapsed() <= timeout {
        if !is_alive_unix(pid) {
            return true;
        }
        thread::sleep(Duration::from_millis(50));
    }
    false
}

#[cfg(unix)]
fn remediate_process_unix(
    identity: &ProcessIdentity,
    term_timeout: Duration,
) -> ProcessRemediationOutcome {
    use nix::sys::signal::{kill, Signal};
    use nix::unistd::Pid;

    let pid = Pid::from_raw(identity.pid as i32);
    if !is_alive_unix(pid) {
        return ProcessRemediationOutcome::AlreadyExited;
    }

    if kill(pid, Some(Signal::SIGTERM)).is_err() {
        return ProcessRemediationOutcome::AlreadyExited;
    }

    if wait_for_exit(pid, term_timeout) {
        return ProcessRemediationOutcome::TerminatedGracefully;
    }

    if kill(pid, Some(Signal::SIGKILL)).is_err() {
        return ProcessRemediationOutcome::AlreadyExited;
    }

    ProcessRemediationOutcome::TerminatedForced
}

#[cfg(windows)]
fn is_alive_windows(pid: u32) -> bool {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{
        GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    };

    const STILL_ACTIVE: u32 = 259;

    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if handle.is_null() {
        return false;
    }

    let mut exit_code: u32 = 0;
    let result = unsafe { GetExitCodeProcess(handle, &mut exit_code) };
    unsafe { CloseHandle(handle) };

    result != 0 && exit_code == STILL_ACTIVE
}

#[cfg(windows)]
fn remediate_process_windows(
    identity: &ProcessIdentity,
    term_timeout: Duration,
) -> ProcessRemediationOutcome {
    use windows_sys::Win32::Foundation::{CloseHandle, WAIT_OBJECT_0};
    use windows_sys::Win32::System::Threading::{
        OpenProcess, TerminateProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE, PROCESS_TERMINATE,
    };

    let pid = identity.pid;
    if !is_alive_windows(pid) {
        return ProcessRemediationOutcome::AlreadyExited;
    }

    let handle = unsafe { OpenProcess(PROCESS_TERMINATE | PROCESS_SYNCHRONIZE, 0, pid) };
    if handle.is_null() {
        return ProcessRemediationOutcome::AlreadyExited;
    }

    let timeout_ms = term_timeout.as_millis().min(u32::MAX as u128) as u32;
    let wait_result = unsafe { WaitForSingleObject(handle, timeout_ms) };

    if wait_result == WAIT_OBJECT_0 {
        unsafe { CloseHandle(handle) };
        return ProcessRemediationOutcome::TerminatedGracefully;
    }

    let success = unsafe { TerminateProcess(handle, 1) };
    unsafe { CloseHandle(handle) };

    if success == 0 {
        return ProcessRemediationOutcome::AlreadyExited;
    }

    ProcessRemediationOutcome::TerminatedForced
}

fn remediate_process(
    identity: &ProcessIdentity,
    ownership_policy: &OwnershipPolicy,
    term_timeout: Duration,
) -> ProcessRemediationOutcome {
    if !matches!(
        validate_process_identity(identity, ownership_policy),
        SafetyDecision::Allowed
    ) {
        return ProcessRemediationOutcome::Rejected;
    }

    #[cfg(unix)]
    {
        return remediate_process_unix(identity, term_timeout);
    }

    #[cfg(windows)]
    {
        return remediate_process_windows(identity, term_timeout);
    }

    #[cfg(not(any(unix, windows)))]
    {
        let _ = (identity, term_timeout);
        ProcessRemediationOutcome::AlreadyExited
    }
}

fn build_rust_analyzer_ownership_policy(policy: &RustAnalyzerMemoryGuardPolicy) -> OwnershipPolicy {
    OwnershipPolicy {
        expected_uid: current_effective_uid(),
        required_command_markers: vec!["rust-analyzer".to_string()],
        same_uid_only: policy.same_uid_only,
    }
}

fn run_rust_analyzer_memory_guard_once(config: &RaGuardConfig) -> MemoryGuardOutput {
    let inventory = ProcessInventory::collect_live_for_rust_analyzer_guard();
    let mut guard = RustAnalyzerMemoryGuard::default();
    run_rust_analyzer_memory_guard_once_with_inventory(
        config,
        &mut guard,
        &inventory,
        SystemTime::now(),
    )
}

fn run_rust_analyzer_memory_guard_once_with_inventory(
    config: &RaGuardConfig,
    guard: &mut RustAnalyzerMemoryGuard,
    inventory: &ProcessInventory,
    now: SystemTime,
) -> MemoryGuardOutput {
    let term_timeout = Duration::from_secs(config.completion.cleanup_retry_interval_secs.max(1));
    let policy = &config.rust_analyzer_memory_guard;
    let ownership_policy = build_rust_analyzer_ownership_policy(policy);
    let candidates = guard.observe_inventory(inventory, policy, &ownership_policy, now);

    if config.mode != Mode::Enforce {
        return MemoryGuardOutput {
            rust_analyzer_memory_candidates: candidates.len(),
            ..MemoryGuardOutput::default()
        };
    }

    let mut remediations = 0;
    for candidate in &candidates {
        let outcome = remediate_process(&candidate.identity, &ownership_policy, term_timeout);
        if remediation_succeeded(&outcome) {
            remediations += 1;
        }
    }

    if remediations > 0 {
        guard.record_remediation(now);
    }

    MemoryGuardOutput {
        rust_analyzer_memory_candidates: candidates.len(),
        rust_analyzer_memory_remediations: remediations,
    }
}

#[cfg(not(target_os = "macos"))]
fn collect_live_rust_analyzer_sysinfo() -> ProcessInventory {
    use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

    let mut system = System::new();
    system.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing()
            .with_memory()
            .with_user(UpdateKind::OnlyIfNotSet)
            .with_cmd(UpdateKind::OnlyIfNotSet),
    );

    let samples = system.processes().values().filter_map(|process| {
        let command = if process.cmd().is_empty() {
            process.name().to_string_lossy().into_owned()
        } else {
            process
                .cmd()
                .iter()
                .map(|part| part.to_string_lossy())
                .collect::<Vec<_>>()
                .join(" ")
        };

        if !command_contains_rust_analyzer(&command) {
            return None;
        }

        let pid = process.pid().as_u32();
        Some(ProcessSample {
            pid,
            parent_pid: process.parent().map(|parent| parent.as_u32()),
            pgid: process_group_id(pid),
            start_time_secs: process.start_time(),
            uid: current_process_uid(process),
            memory_bytes: process.memory(),
            command,
        })
    });

    ProcessInventory::from_samples(samples)
}

#[cfg(not(target_os = "macos"))]
#[cfg(unix)]
fn current_process_uid(process: &sysinfo::Process) -> Option<u32> {
    process.user_id().map(|uid| **uid)
}

#[cfg(not(target_os = "macos"))]
#[cfg(not(unix))]
fn current_process_uid(_process: &sysinfo::Process) -> Option<u32> {
    None
}

#[cfg(target_os = "macos")]
fn collect_live_rust_analyzer_macos() -> Option<ProcessInventory> {
    let output = std::process::Command::new("ps")
        .args(["-axo", "pid=,ppid=,pgid=,uid=,rss=,etime=,command="])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8(output.stdout).ok()?;
    let now_unix_secs = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .ok()?
        .as_secs();

    let samples = stdout
        .lines()
        .filter_map(|line| parse_rust_analyzer_ps_line(line, now_unix_secs));
    Some(ProcessInventory::from_samples(samples))
}

#[cfg(target_os = "macos")]
fn parse_rust_analyzer_ps_line(line: &str, now_unix_secs: u64) -> Option<ProcessSample> {
    let mut fields = line.split_whitespace();

    let pid = fields.next()?.parse::<u32>().ok()?;
    let parent_pid = fields.next()?.parse::<u32>().ok()?;
    let pgid = fields.next()?.parse::<u32>().ok()?;
    let uid = fields.next()?.parse::<u32>().ok()?;
    let rss_kib = fields.next()?.parse::<u64>().ok()?;
    let elapsed = parse_elapsed_secs(fields.next()?)?;
    let command = fields.collect::<Vec<_>>().join(" ");

    if command.is_empty() || !command_contains_rust_analyzer(&command) {
        return None;
    }

    Some(ProcessSample {
        pid,
        parent_pid: Some(parent_pid),
        pgid: Some(pgid),
        start_time_secs: now_unix_secs.saturating_sub(elapsed),
        uid: Some(uid),
        memory_bytes: rss_kib.saturating_mul(1024),
        command,
    })
}

fn parse_elapsed_secs(raw: &str) -> Option<u64> {
    let (days, hms) = match raw.split_once('-') {
        Some((days, hms)) => (days.parse::<u64>().ok()?, hms),
        None => (0, raw),
    };

    let parts: Vec<_> = hms.split(':').collect();
    let (hours, minutes, seconds) = match parts.as_slice() {
        [minutes, seconds] => (
            0,
            minutes.parse::<u64>().ok()?,
            seconds.parse::<u64>().ok()?,
        ),
        [hours, minutes, seconds] => (
            hours.parse::<u64>().ok()?,
            minutes.parse::<u64>().ok()?,
            seconds.parse::<u64>().ok()?,
        ),
        _ => return None,
    };

    Some(days * 24 * 60 * 60 + hours * 60 * 60 + minutes * 60 + seconds)
}

fn main() -> ExitCode {
    let (config_path, json) = match parse_args(std::env::args_os()) {
        Ok(parsed) => parsed,
        Err(error) => {
            eprintln!("error: {error}");
            return ExitCode::from(2);
        }
    };

    let config = match load_ra_guard_config(&config_path) {
        Ok(config) => config,
        Err(error) => {
            eprintln!("error: config load failure: {error}");
            return ExitCode::from(1);
        }
    };

    let output = run_rust_analyzer_memory_guard_once(&config);

    let rendered = render_output(&output, json);
    println!("{rendered}");
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::path::PathBuf;
    use std::time::{Duration, UNIX_EPOCH};

    use super::{
        parse_args, parse_ra_guard_config, run_rust_analyzer_memory_guard_once_with_inventory,
        CompletionCleanupPolicy, MemoryGuardOutput, Mode, ProcessInventory, ProcessSample,
        RaGuardConfig, RustAnalyzerMemoryGuard, RustAnalyzerMemoryGuardPolicy,
    };

    #[test]
    fn parse_args_accepts_minimal_shape() {
        let parsed = parse_args(vec![
            OsString::from("cancerbroker-ra-guard"),
            OsString::from("--config"),
            OsString::from("fixtures/config/rust-analyzer-guard-minimal.toml"),
        ])
        .expect("args should parse");

        assert_eq!(
            parsed,
            (
                PathBuf::from("fixtures/config/rust-analyzer-guard-minimal.toml"),
                false
            )
        );
    }

    #[test]
    fn parse_args_accepts_json_flag() {
        let parsed = parse_args(vec![
            OsString::from("cancerbroker-ra-guard"),
            OsString::from("--config"),
            OsString::from("/tmp/guardian.toml"),
            OsString::from("--json"),
        ])
        .expect("args should parse");

        assert_eq!(parsed, (PathBuf::from("/tmp/guardian.toml"), true));
    }

    #[test]
    fn parse_args_rejects_unsupported_flags() {
        let error = parse_args(vec![
            OsString::from("cancerbroker-ra-guard"),
            OsString::from("--config"),
            OsString::from("/tmp/guardian.toml"),
            OsString::from("--max-events"),
        ])
        .expect_err("unsupported flag should fail");

        assert!(error.contains("--json"));
    }

    #[test]
    fn config_defaults_match_ra_guard_expectations() {
        let config: RaGuardConfig = parse_ra_guard_config("mode = \"observe\"\n")
            .expect("config should parse with defaults");

        assert_eq!(config.mode, Mode::Observe);
        assert_eq!(
            config.rust_analyzer_memory_guard,
            RustAnalyzerMemoryGuardPolicy::default()
        );
        assert_eq!(
            config.completion,
            CompletionCleanupPolicy {
                cleanup_retry_interval_secs: 15,
            }
        );
    }

    #[test]
    fn guard_boundary_reports_candidates_in_observe_mode() {
        let config = RaGuardConfig {
            mode: Mode::Observe,
            rust_analyzer_memory_guard: RustAnalyzerMemoryGuardPolicy {
                enabled: true,
                max_rss_bytes: 100,
                required_consecutive_samples: 2,
                startup_grace_secs: 0,
                cooldown_secs: 300,
                same_uid_only: false,
            },
            completion: CompletionCleanupPolicy {
                cleanup_retry_interval_secs: 1,
            },
        };
        let now = UNIX_EPOCH + Duration::from_secs(500);
        let mut guard = RustAnalyzerMemoryGuard::default();

        assert_eq!(
            run_rust_analyzer_memory_guard_once_with_inventory(
                &config,
                &mut guard,
                &ProcessInventory::from_samples([ProcessSample {
                    pid: 77,
                    parent_pid: Some(1),
                    pgid: Some(77),
                    start_time_secs: 42,
                    uid: Some(999_999),
                    memory_bytes: 110,
                    command: "rust-analyzer".to_string(),
                }]),
                now,
            ),
            MemoryGuardOutput::default()
        );

        let output = run_rust_analyzer_memory_guard_once_with_inventory(
            &config,
            &mut guard,
            &ProcessInventory::from_samples([ProcessSample {
                pid: 77,
                parent_pid: Some(1),
                pgid: Some(77),
                start_time_secs: 42,
                uid: Some(999_999),
                memory_bytes: 120,
                command: "rust-analyzer".to_string(),
            }]),
            now,
        );

        assert_eq!(
            output,
            MemoryGuardOutput {
                rust_analyzer_memory_candidates: 1,
                rust_analyzer_memory_remediations: 0,
            }
        );
    }
}
