use std::collections::{BTreeMap, HashMap};
#[cfg(target_os = "macos")]
use std::process::Command;
#[cfg(target_os = "macos")]
use std::time::SystemTime;

#[derive(Debug, Clone, PartialEq)]
pub struct ProcessSample {
    pub pid: u32,
    pub parent_pid: Option<u32>,
    pub pgid: Option<u32>,
    pub start_time_secs: u64,
    pub uid: Option<u32>,
    pub memory_bytes: u64,
    pub cpu_percent: f32,
    pub command: String,
    pub listening_ports: Vec<u16>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProcessFingerprint {
    pub start_time_secs: u64,
    pub command: String,
}

#[derive(Debug, Default, Clone)]
pub struct ProcessInventory {
    processes: BTreeMap<u32, ProcessSample>,
    children_by_parent: BTreeMap<u32, Vec<u32>>,
}

fn normalize_children(children_by_parent: &mut BTreeMap<u32, Vec<u32>>) {
    for children in children_by_parent.values_mut() {
        children.sort_unstable();
        children.dedup();
    }
}

use crate::platform::process_group_id;

fn refresh_processes(system: &mut sysinfo::System) {
    use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, UpdateKind};

    system.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing()
            .with_memory()
            .with_cpu()
            .with_user(UpdateKind::OnlyIfNotSet)
            .with_cmd(UpdateKind::OnlyIfNotSet),
    );
}

fn command_from_sysinfo(process: &sysinfo::Process) -> String {
    if process.cmd().is_empty() {
        process.name().to_string_lossy().into_owned()
    } else {
        process
            .cmd()
            .iter()
            .map(|part| part.to_string_lossy())
            .collect::<Vec<_>>()
            .join(" ")
    }
}

fn listening_ports_by_pid() -> HashMap<u32, Vec<u16>> {
    use netstat2::{
        AddressFamilyFlags, ProtocolFlags, ProtocolSocketInfo, TcpState, get_sockets_info,
    };

    let af_flags = AddressFamilyFlags::IPV4 | AddressFamilyFlags::IPV6;
    let proto_flags = ProtocolFlags::TCP | ProtocolFlags::UDP;

    let sockets = match get_sockets_info(af_flags, proto_flags) {
        Ok(sockets) => sockets,
        Err(_) => return HashMap::new(),
    };

    let mut ports_by_pid: HashMap<u32, Vec<u16>> = HashMap::new();
    for si in sockets {
        let port = match si.protocol_socket_info {
            ProtocolSocketInfo::Tcp(ref tcp_si) if tcp_si.state == TcpState::Listen => {
                (tcp_si.local_port > 0).then_some(tcp_si.local_port)
            }
            ProtocolSocketInfo::Udp(ref udp_si) => {
                (udp_si.local_port > 0).then_some(udp_si.local_port)
            }
            _ => None,
        };

        let Some(port) = port else {
            continue;
        };

        for pid in si.associated_pids {
            ports_by_pid.entry(pid).or_default().push(port);
        }
    }

    for ports in ports_by_pid.values_mut() {
        ports.sort_unstable();
        ports.dedup();
    }

    ports_by_pid
}

fn build_process_fingerprint(process: &ProcessSample) -> ProcessFingerprint {
    ProcessFingerprint {
        start_time_secs: process.start_time_secs,
        command: process.command.clone(),
    }
}

impl ProcessInventory {
    pub fn from_samples(samples: impl IntoIterator<Item = ProcessSample>) -> Self {
        let mut processes = BTreeMap::new();
        let mut children_by_parent: BTreeMap<u32, Vec<u32>> = BTreeMap::new();

        for sample in samples {
            if let Some(parent_pid) = sample.parent_pid {
                children_by_parent
                    .entry(parent_pid)
                    .or_default()
                    .push(sample.pid);
            }
            processes.insert(sample.pid, sample);
        }

        normalize_children(&mut children_by_parent);

        Self {
            processes,
            children_by_parent,
        }
    }

    pub fn sample(&self, pid: u32) -> Option<&ProcessSample> {
        self.processes.get(&pid)
    }

    pub fn samples(&self) -> impl Iterator<Item = &ProcessSample> {
        self.processes.values()
    }

    pub fn children_of(&self, parent_pid: u32) -> Vec<u32> {
        self.children_by_parent
            .get(&parent_pid)
            .cloned()
            .unwrap_or_default()
    }

    pub fn process_fingerprint(&self, pid: u32) -> Option<ProcessFingerprint> {
        self.processes.get(&pid).map(build_process_fingerprint)
    }

    pub fn is_same_process_instance(&self, pid: u32, start_time_secs: u64) -> bool {
        self.processes
            .get(&pid)
            .map(|process| process.start_time_secs == start_time_secs)
            .unwrap_or(false)
    }

    pub fn total_memory_bytes(&self) -> u64 {
        self.processes
            .values()
            .map(|process| process.memory_bytes)
            .sum()
    }

    pub fn collect_live() -> Self {
        use sysinfo::System;

        let mut system = System::new();
        Self::collect_live_with(&mut system)
    }

    pub fn collect_live_for_rust_analyzer_guard() -> Self {
        #[cfg(target_os = "macos")]
        {
            if let Some(inventory) = collect_live_rust_analyzer_macos() {
                return inventory;
            }
        }

        Self::collect_live()
    }

    pub fn collect_live_with(system: &mut sysinfo::System) -> Self {
        refresh_processes(system);
        let ports_by_pid = listening_ports_by_pid();

        Self::from_samples(system.processes().values().map(|process| {
            let command = command_from_sysinfo(process);

            let pid_u32 = process.pid().as_u32();
            let pgid = process_group_id(pid_u32);
            let listening_ports = ports_by_pid.get(&pid_u32).cloned().unwrap_or_default();

            ProcessSample {
                pid: pid_u32,
                parent_pid: process.parent().map(|pid| pid.as_u32()),
                pgid,
                start_time_secs: process.start_time(),
                uid: current_process_uid(process),
                memory_bytes: process.memory(),
                cpu_percent: process.cpu_usage(),
                command,
                listening_ports,
            }
        }))
    }
}

#[cfg(target_os = "macos")]
fn collect_live_rust_analyzer_macos() -> Option<ProcessInventory> {
    let output = Command::new("ps")
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

    if command.is_empty() || !command.to_ascii_lowercase().contains("rust-analyzer") {
        return None;
    }

    Some(ProcessSample {
        pid,
        parent_pid: Some(parent_pid),
        pgid: Some(pgid),
        start_time_secs: now_unix_secs.saturating_sub(elapsed),
        uid: Some(uid),
        memory_bytes: rss_kib.saturating_mul(1024),
        cpu_percent: 0.0,
        command,
        listening_ports: Vec::new(),
    })
}

#[cfg(any(target_os = "macos", test))]
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

#[cfg(unix)]
fn current_process_uid(process: &sysinfo::Process) -> Option<u32> {
    process.user_id().map(|uid| **uid)
}

#[cfg(not(unix))]
fn current_process_uid(_process: &sysinfo::Process) -> Option<u32> {
    None
}

#[cfg(test)]
mod tests {
    use super::{ProcessFingerprint, ProcessInventory, ProcessSample, build_process_fingerprint};

    #[test]
    fn parse_elapsed_secs_supports_mm_ss() {
        assert_eq!(super::parse_elapsed_secs("03:15"), Some(195));
    }

    #[test]
    fn parse_elapsed_secs_supports_hh_mm_ss() {
        assert_eq!(super::parse_elapsed_secs("01:02:03"), Some(3723));
    }

    #[test]
    fn parse_elapsed_secs_supports_dd_hh_mm_ss() {
        assert_eq!(super::parse_elapsed_secs("2-01:02:03"), Some(176_523));
    }

    #[test]
    fn parse_elapsed_secs_rejects_invalid_formats() {
        assert_eq!(super::parse_elapsed_secs("invalid"), None);
        assert_eq!(super::parse_elapsed_secs("1:2:3:4"), None);
    }

    fn sample_processes() -> Vec<ProcessSample> {
        vec![
            ProcessSample {
                pid: 10,
                parent_pid: Some(1),
                pgid: Some(10),
                start_time_secs: 100,
                uid: Some(501),
                memory_bytes: 512,
                cpu_percent: 0.5,
                command: "opencode ses_alpha worker".to_string(),
                listening_ports: vec![],
            },
            ProcessSample {
                pid: 11,
                parent_pid: Some(1),
                pgid: Some(11),
                start_time_secs: 110,
                uid: Some(501),
                memory_bytes: 256,
                cpu_percent: 0.2,
                command: "opencode ses_beta child".to_string(),
                listening_ports: vec![],
            },
            ProcessSample {
                pid: 12,
                parent_pid: Some(1),
                pgid: Some(12),
                start_time_secs: 120,
                uid: Some(501),
                memory_bytes: 128,
                cpu_percent: 0.1,
                command: "opencode ses_beta child-duplicate-parent".to_string(),
                listening_ports: vec![],
            },
        ]
    }

    #[test]
    fn from_samples_normalizes_duplicate_children() {
        let mut processes = sample_processes();
        processes.push(ProcessSample {
            pid: 11,
            parent_pid: Some(1),
            pgid: Some(11),
            start_time_secs: 111,
            uid: Some(501),
            memory_bytes: 300,
            cpu_percent: 0.3,
            command: "opencode ses_beta child-replaced".to_string(),
            listening_ports: vec![],
        });

        let inventory = ProcessInventory::from_samples(processes);

        assert_eq!(inventory.children_of(1), vec![10, 11, 12]);
        assert_eq!(
            inventory
                .sample(11)
                .expect("pid 11 should exist")
                .memory_bytes,
            300
        );
    }

    #[test]
    fn build_process_fingerprint_copies_identity_fields() {
        let sample = sample_processes().remove(0);

        assert_eq!(
            build_process_fingerprint(&sample),
            ProcessFingerprint {
                start_time_secs: 100,
                command: "opencode ses_alpha worker".to_string(),
            }
        );
    }

    #[test]
    fn process_fingerprint_and_instance_checks_follow_stored_sample() {
        let inventory = ProcessInventory::from_samples(sample_processes());

        assert_eq!(
            inventory.process_fingerprint(10),
            Some(ProcessFingerprint {
                start_time_secs: 100,
                command: "opencode ses_alpha worker".to_string(),
            })
        );
        assert!(inventory.is_same_process_instance(10, 100));
        assert!(!inventory.is_same_process_instance(10, 101));
        assert_eq!(inventory.process_fingerprint(999), None);
    }

    #[test]
    fn total_memory_bytes_sums_all_samples() {
        let inventory = ProcessInventory::from_samples(sample_processes());

        assert_eq!(inventory.total_memory_bytes(), 896);
    }

    #[test]
    fn collect_live_includes_current_process() {
        let inventory = ProcessInventory::collect_live();
        let current_pid = std::process::id();
        let sample = inventory
            .sample(current_pid)
            .expect("current process should be present in live inventory");

        assert_eq!(sample.pid, current_pid);
        assert!(!sample.command.is_empty());
        assert!(inventory.total_memory_bytes() >= sample.memory_bytes);
        assert_eq!(
            inventory.process_fingerprint(current_pid),
            Some(ProcessFingerprint {
                start_time_secs: sample.start_time_secs,
                command: sample.command.clone(),
            })
        );
    }
}
