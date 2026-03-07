use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq)]
pub struct ProcessSample {
    pub pid: u32,
    pub parent_pid: Option<u32>,
    pub start_time_secs: u64,
    pub uid: Option<u32>,
    pub memory_bytes: u64,
    pub cpu_percent: f32,
    pub command: String,
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

        for children in children_by_parent.values_mut() {
            children.sort_unstable();
            children.dedup();
        }

        Self {
            processes,
            children_by_parent,
        }
    }

    pub fn sample(&self, pid: u32) -> Option<&ProcessSample> {
        self.processes.get(&pid)
    }

    pub fn children_of(&self, parent_pid: u32) -> Vec<u32> {
        self.children_by_parent
            .get(&parent_pid)
            .cloned()
            .unwrap_or_default()
    }

    pub fn process_fingerprint(&self, pid: u32) -> Option<ProcessFingerprint> {
        self.processes.get(&pid).map(|process| ProcessFingerprint {
            start_time_secs: process.start_time_secs,
            command: process.command.clone(),
        })
    }

    pub fn is_same_process_instance(&self, pid: u32, start_time_secs: u64) -> bool {
        self.processes
            .get(&pid)
            .map(|process| process.start_time_secs == start_time_secs)
            .unwrap_or(false)
    }
}
