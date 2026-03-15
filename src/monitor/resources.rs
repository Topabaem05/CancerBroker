use std::collections::BTreeMap;

use serde::Serialize;
#[cfg(unix)]
use std::process::Command;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OpenResource {
    pub descriptor: String,
    pub kind: String,
    pub target: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProcessResourceReport {
    pub pid: u32,
    pub open_files: Vec<String>,
    pub open_connections: Vec<String>,
    pub other_resources: Vec<String>,
    pub resources: Vec<OpenResource>,
    pub collection_error: Option<String>,
}

impl ProcessResourceReport {
    pub fn empty(pid: u32) -> Self {
        Self {
            pid,
            open_files: Vec::new(),
            open_connections: Vec::new(),
            other_resources: Vec::new(),
            resources: Vec::new(),
            collection_error: None,
        }
    }
}

#[cfg(any(unix, test))]
fn finalize_resource(report: &mut ProcessResourceReport, resource: OpenResource) {
    let classification = if resource.target.starts_with('/') {
        Some(&mut report.open_files)
    } else if resource.target.contains("->")
        || resource.kind.starts_with("IPv")
        || resource.kind == "unix"
    {
        Some(&mut report.open_connections)
    } else {
        Some(&mut report.other_resources)
    };

    if let Some(bucket) = classification {
        bucket.push(resource.target.clone());
    }
    report.resources.push(resource);
}

#[cfg(test)]
fn parse_lsof_field_output(output: &str, pid: u32) -> ProcessResourceReport {
    parse_lsof_field_output_batch(output)
        .remove(&pid)
        .unwrap_or_else(|| ProcessResourceReport::empty(pid))
}

#[cfg(any(unix, test))]
fn flush_resource(
    reports: &mut BTreeMap<u32, ProcessResourceReport>,
    current_pid: Option<u32>,
    descriptor: &mut Option<String>,
    kind: &mut Option<String>,
    target: &mut Option<String>,
    state: &mut Option<String>,
) {
    let Some(pid) = current_pid else {
        return;
    };
    let Some(descriptor_value) = descriptor.take() else {
        return;
    };

    let target_value = target.take().unwrap_or_else(|| "(unknown)".to_string());
    let full_target = match state.take() {
        Some(state_value) => format!("{target_value} [{state_value}]"),
        None => target_value,
    };
    finalize_resource(
        reports
            .entry(pid)
            .or_insert_with(|| ProcessResourceReport::empty(pid)),
        OpenResource {
            descriptor: descriptor_value,
            kind: kind.take().unwrap_or_else(|| "unknown".to_string()),
            target: full_target,
        },
    );
}

#[cfg(any(unix, test))]
fn parse_lsof_field_output_batch(output: &str) -> BTreeMap<u32, ProcessResourceReport> {
    let mut reports = BTreeMap::new();
    let mut current_pid: Option<u32> = None;
    let mut current_descriptor: Option<String> = None;
    let mut current_kind: Option<String> = None;
    let mut current_target: Option<String> = None;
    let mut socket_state: Option<String> = None;

    for line in output.lines() {
        if line.is_empty() {
            continue;
        }

        if let Some(rest) = line.strip_prefix('p') {
            flush_resource(
                &mut reports,
                current_pid,
                &mut current_descriptor,
                &mut current_kind,
                &mut current_target,
                &mut socket_state,
            );

            current_pid = rest.parse::<u32>().ok();
            if let Some(pid) = current_pid {
                reports
                    .entry(pid)
                    .or_insert_with(|| ProcessResourceReport::empty(pid));
            }
            continue;
        }

        if let Some(rest) = line.strip_prefix('f') {
            flush_resource(
                &mut reports,
                current_pid,
                &mut current_descriptor,
                &mut current_kind,
                &mut current_target,
                &mut socket_state,
            );
            current_descriptor = Some(rest.to_string());
            continue;
        }

        if let Some(rest) = line.strip_prefix('t') {
            current_kind = Some(rest.to_string());
            continue;
        }

        if let Some(rest) = line.strip_prefix('n') {
            current_target = Some(rest.to_string());
            continue;
        }

        if let Some(rest) = line.strip_prefix('T')
            && let Some(state) = rest.strip_prefix("ST=")
        {
            socket_state = Some(state.to_string());
        }
    }

    flush_resource(
        &mut reports,
        current_pid,
        &mut current_descriptor,
        &mut current_kind,
        &mut current_target,
        &mut socket_state,
    );

    reports
}

#[cfg(unix)]
pub fn collect_process_resources(pid: u32) -> ProcessResourceReport {
    collect_process_resources_batch(&[pid])
        .remove(&pid)
        .unwrap_or_else(|| ProcessResourceReport::empty(pid))
}

#[cfg(unix)]
pub fn collect_process_resources_batch(pids: &[u32]) -> BTreeMap<u32, ProcessResourceReport> {
    if pids.is_empty() {
        return BTreeMap::new();
    }

    let pid_arg = pids
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(",");
    let output = match Command::new("lsof")
        .args(["-n", "-P", "-F", "pftnT", "-p", pid_arg.as_str()])
        .output()
    {
        Ok(output) => output,
        Err(error) => {
            return resource_error_reports(pids, format!("lsof execution failed: {error}"));
        }
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let error = if stderr.is_empty() {
            format!("lsof exited with status {}", output.status)
        } else {
            stderr
        };
        return resource_error_reports(pids, error);
    }

    let mut reports = parse_lsof_field_output_batch(&String::from_utf8_lossy(&output.stdout));
    for &pid in pids {
        reports
            .entry(pid)
            .or_insert_with(|| ProcessResourceReport::empty(pid));
    }
    reports
}

#[cfg(not(unix))]
pub fn collect_process_resources(pid: u32) -> ProcessResourceReport {
    let mut report = ProcessResourceReport::empty(pid);
    report.collection_error =
        Some("open resource analysis unsupported on this platform".to_string());
    report
}

#[cfg(not(unix))]
pub fn collect_process_resources_batch(pids: &[u32]) -> BTreeMap<u32, ProcessResourceReport> {
    pids.iter()
        .copied()
        .map(|pid| (pid, collect_process_resources(pid)))
        .collect()
}

#[cfg(unix)]
fn resource_error_reports(pids: &[u32], error: String) -> BTreeMap<u32, ProcessResourceReport> {
    pids.iter()
        .copied()
        .map(|pid| {
            let mut report = ProcessResourceReport::empty(pid);
            report.collection_error = Some(error.clone());
            (pid, report)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{
        ProcessResourceReport, collect_process_resources, parse_lsof_field_output,
        parse_lsof_field_output_batch,
    };

    #[test]
    fn parse_lsof_field_output_classifies_files_and_connections() {
        let report = parse_lsof_field_output(
            "p123\nfcwd\ntVDIR\nn/Users/guribbong/code/cancerbroker\nf21\ntVREG\nn/Users/guribbong/.local/share/opencode/opencode.db\nf34\ntIPv4\nn172.30.1.87:53682->104.26.9.108:443\nTST=ESTABLISHED\nf40\ntKQUEUE\nn[count=0, state=0x12]\n",
            123,
        );

        assert_eq!(report.pid, 123);
        assert_eq!(report.open_files.len(), 2);
        assert_eq!(report.open_connections.len(), 1);
        assert_eq!(report.other_resources.len(), 1);
        assert!(report.open_files[0].contains("/Users/guribbong/code/cancerbroker"));
        assert!(report.open_connections[0].contains("ESTABLISHED"));
    }

    #[test]
    fn parse_lsof_field_output_keeps_descriptor_records() {
        let report = parse_lsof_field_output("p55\nfcwd\ntVDIR\nn/tmp\n", 55);

        assert_eq!(report.resources.len(), 1);
        assert_eq!(report.resources[0].descriptor, "cwd");
        assert_eq!(report.resources[0].kind, "VDIR");
        assert_eq!(report.resources[0].target, "/tmp");
    }

    #[test]
    fn parse_lsof_field_output_batch_keeps_resources_separated_by_pid() {
        let reports = parse_lsof_field_output_batch(
            "p55\nfcwd\ntVDIR\nn/tmp\np77\nf10\ntIPv4\nn127.0.0.1:3000\nTST=LISTEN\n",
        );

        assert_eq!(reports[&55].open_files, vec!["/tmp"]);
        assert!(reports[&55].open_connections.is_empty());
        assert_eq!(
            reports[&77].open_connections,
            vec!["127.0.0.1:3000 [LISTEN]"]
        );
    }

    #[test]
    fn collect_process_resources_returns_current_process_report_shape() {
        let report = collect_process_resources(std::process::id());

        assert_eq!(report.pid, std::process::id());
    }

    #[test]
    fn empty_report_starts_without_resources() {
        let report = ProcessResourceReport::empty(9);

        assert_eq!(report.pid, 9);
        assert!(report.resources.is_empty());
        assert_eq!(report.collection_error, None);
    }
}
