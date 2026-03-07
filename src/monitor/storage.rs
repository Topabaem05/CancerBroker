use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

#[derive(Debug, Clone)]
pub struct ArtifactRecord {
    pub path: PathBuf,
    pub bytes: u64,
    pub modified_at: SystemTime,
}

#[derive(Debug, Clone, Default)]
pub struct StorageSnapshot {
    pub artifacts: Vec<ArtifactRecord>,
    pub total_bytes: u64,
}

pub fn scan_allowlisted_roots(allowlisted_roots: &[PathBuf]) -> std::io::Result<StorageSnapshot> {
    let mut artifacts = Vec::new();
    let mut total_bytes = 0_u64;

    for root in allowlisted_roots {
        if !root.exists() {
            continue;
        }

        walk_files(root, &mut artifacts)?;
    }

    artifacts.sort_by(|a, b| a.path.cmp(&b.path));
    artifacts.dedup_by(|left, right| left.path == right.path);

    for artifact in &artifacts {
        total_bytes = total_bytes.saturating_add(artifact.bytes);
    }

    Ok(StorageSnapshot {
        artifacts,
        total_bytes,
    })
}

pub fn merge_watch_events_with_scan(
    watch_seen_paths: &BTreeSet<PathBuf>,
    scan: &StorageSnapshot,
) -> BTreeSet<PathBuf> {
    let mut merged = watch_seen_paths.clone();

    for artifact in &scan.artifacts {
        merged.insert(artifact.path.clone());
    }

    merged
}

pub fn stale_artifacts(
    snapshot: &StorageSnapshot,
    now: SystemTime,
    max_age: Duration,
) -> Vec<PathBuf> {
    snapshot
        .artifacts
        .iter()
        .filter_map(|artifact| {
            now.duration_since(artifact.modified_at)
                .ok()
                .filter(|age| *age >= max_age)
                .map(|_| artifact.path.clone())
        })
        .collect()
}

fn walk_files(dir: &Path, artifacts: &mut Vec<ArtifactRecord>) -> std::io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let metadata = entry.metadata()?;

        if metadata.is_dir() {
            walk_files(&path, artifacts)?;
            continue;
        }

        if metadata.is_file() {
            artifacts.push(ArtifactRecord {
                path,
                bytes: metadata.len(),
                modified_at: metadata.modified()?,
            });
        }
    }

    Ok(())
}
