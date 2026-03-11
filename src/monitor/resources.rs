use std::process::Command;

use serde::Serialize;

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

fn parse_lsof_field_output(output: &str, pid: u32) -> ProcessResourceReport {
    let mut report = ProcessResourceReport::empty(pid);
    let mut current_descriptor: Option<String> = None;
    let mut current_kind: Option<String> = None;
    let mut current_target: Option<String> = None;
    let mut socket_state: Option<String> = None;

    let flush = |report: &mut ProcessResourceReport,
                 descriptor: &mut Option<String>,
                 kind: &mut Option<String>,
                 target: &mut Option<String>,
                 state: &mut Option<String>| {
        let Some(descriptor_value) = descriptor.take() else {
            return;
        };
        let target_value = target.take().unwrap_or_else(|| "(unknown)".to_string());
        let full_target = match state.take() {
            Some(state_value) => format!("{target_value} [{state_value}]"),
            None => target_value,
        };
        finalize_resource(
            report,
            OpenResource {
                descriptor: descriptor_value,
                kind: kind.take().unwrap_or_else(|| "unknown".to_string()),
                target: full_target,
            },
        );
    };

    for line in output.lines() {
        if line.is_empty() {
            continue;
        }

        if let Some(rest) = line.strip_prefix('p') {
            if let Ok(parsed_pid) = rest.parse::<u32>()
                && parsed_pid != pid
            {
                continue;
            }
            continue;
        }

        if let Some(rest) = line.strip_prefix('f') {
            flush(
                &mut report,
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

    flush(
        &mut report,
        &mut current_descriptor,
        &mut current_kind,
        &mut current_target,
        &mut socket_state,
    );
    report
}

#[cfg(unix)]
pub fn collect_process_resources(pid: u32) -> ProcessResourceReport {
    let output = match Command::new("lsof")
        .args(["-n", "-P", "-F", "pftnT", "-p", &pid.to_string()])
        .output()
    {
        Ok(output) => output,
        Err(error) => {
            let mut report = ProcessResourceReport::empty(pid);
            report.collection_error = Some(format!("lsof execution failed: {error}"));
            return report;
        }
    };

    if !output.status.success() {
        let mut report = ProcessResourceReport::empty(pid);
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        report.collection_error = Some(if stderr.is_empty() {
            format!("lsof exited with status {}", output.status)
        } else {
            stderr
        });
        return report;
    }

    parse_lsof_field_output(&String::from_utf8_lossy(&output.stdout), pid)
}

#[cfg(not(unix))]
pub fn collect_process_resources(pid: u32) -> ProcessResourceReport {
    let mut report = ProcessResourceReport::empty(pid);
    report.collection_error =
        Some("open resource analysis unsupported on this platform".to_string());
    report
}

#[cfg(test)]
mod tests {
    use super::{ProcessResourceReport, collect_process_resources, parse_lsof_field_output};

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
