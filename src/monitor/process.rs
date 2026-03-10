use std::collections::BTreeMap;

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

fn get_pgid(pid: u32) -> Option<u32> {
    use nix::unistd::{getpgid, Pid};

    getpgid(Some(Pid::from_raw(pid as i32)))
        .ok()
        .map(|pgid| pgid.as_raw() as u32)
}

fn get_listening_ports(pid: u32) -> Vec<u16> {
    use netstat2::{
        get_sockets_info, AddressFamilyFlags, ProtocolFlags, ProtocolSocketInfo, TcpState,
    };

    let af_flags = AddressFamilyFlags::IPV4 | AddressFamilyFlags::IPV6;
    let proto_flags = ProtocolFlags::TCP | ProtocolFlags::UDP;

    let sockets = match get_sockets_info(af_flags, proto_flags) {
        Ok(sockets) => sockets,
        Err(_) => return Vec::new(),
    };

    let mut ports = Vec::new();
    for si in sockets {
        if !si.associated_pids.contains(&pid) {
            continue;
        }
        match si.protocol_socket_info {
            ProtocolSocketInfo::Tcp(ref tcp_si) if tcp_si.state == TcpState::Listen => {
                if tcp_si.local_port > 0 {
                    ports.push(tcp_si.local_port);
                }
            }
            ProtocolSocketInfo::Udp(ref udp_si) => {
                if udp_si.local_port > 0 {
                    ports.push(udp_si.local_port);
                }
            }
            _ => {}
        }
    }

    ports.sort_unstable();
    ports.dedup();
    ports
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
        use sysinfo::{ProcessesToUpdate, System};

        let mut system = System::new_all();
        system.refresh_processes(ProcessesToUpdate::All, true);

        Self::from_samples(system.processes().values().map(|process| {
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

            let pid_u32 = process.pid().as_u32();
            let pgid = get_pgid(pid_u32);
            let listening_ports = get_listening_ports(pid_u32);

            ProcessSample {
                pid: pid_u32,
                parent_pid: process.parent().map(|pid| pid.as_u32()),
                pgid,
                start_time_secs: process.start_time(),
                uid: process.user_id().map(|uid| **uid),
                memory_bytes: process.memory(),
                cpu_percent: process.cpu_usage(),
                command,
                listening_ports,
            }
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::{build_process_fingerprint, ProcessFingerprint, ProcessInventory, ProcessSample};

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
