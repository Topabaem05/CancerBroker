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

fn finalize_snapshot(mut artifacts: Vec<ArtifactRecord>) -> StorageSnapshot {
    artifacts.sort_by(|a, b| a.path.cmp(&b.path));
    artifacts.dedup_by(|left, right| left.path == right.path);

    let total_bytes = artifacts.iter().fold(0_u64, |total, artifact| {
        total.saturating_add(artifact.bytes)
    });

    StorageSnapshot {
        artifacts,
        total_bytes,
    }
}

fn stale_artifact_path(
    artifact: &ArtifactRecord,
    now: SystemTime,
    max_age: Duration,
) -> Option<PathBuf> {
    now.duration_since(artifact.modified_at)
        .ok()
        .filter(|age| *age >= max_age)
        .map(|_| artifact.path.clone())
}

pub fn scan_allowlisted_roots(allowlisted_roots: &[PathBuf]) -> std::io::Result<StorageSnapshot> {
    let mut artifacts = Vec::new();

    for root in allowlisted_roots {
        if !root.exists() {
            continue;
        }

        walk_files(root, &mut artifacts)?;
    }

    Ok(finalize_snapshot(artifacts))
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
        .filter_map(|artifact| stale_artifact_path(artifact, now, max_age))
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

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::time::{Duration, UNIX_EPOCH};

    use tempfile::tempdir;

    use super::{
        ArtifactRecord, StorageSnapshot, finalize_snapshot, merge_watch_events_with_scan,
        scan_allowlisted_roots, stale_artifact_path, stale_artifacts,
    };

    #[test]
    fn finalize_snapshot_sorts_dedupes_and_sums_bytes() {
        let snapshot = finalize_snapshot(vec![
            ArtifactRecord {
                path: "/tmp/b.json".into(),
                bytes: 7,
                modified_at: UNIX_EPOCH,
            },
            ArtifactRecord {
                path: "/tmp/a.json".into(),
                bytes: 5,
                modified_at: UNIX_EPOCH,
            },
            ArtifactRecord {
                path: "/tmp/a.json".into(),
                bytes: 99,
                modified_at: UNIX_EPOCH,
            },
        ]);

        assert_eq!(snapshot.artifacts.len(), 2);
        assert_eq!(snapshot.artifacts[0].path.to_string_lossy(), "/tmp/a.json");
        assert_eq!(snapshot.artifacts[1].path.to_string_lossy(), "/tmp/b.json");
        assert_eq!(snapshot.total_bytes, 12);
    }

    #[test]
    fn scan_allowlisted_roots_walks_nested_files_and_ignores_missing_roots() {
        let dir = tempdir().expect("tempdir");
        let nested = dir.path().join("nested");
        fs::create_dir_all(&nested).expect("nested dir should exist");
        fs::write(dir.path().join("a.json"), "abc").expect("root artifact should exist");
        fs::write(nested.join("b.json"), "abcd").expect("nested artifact should exist");

        let snapshot =
            scan_allowlisted_roots(&[dir.path().join("missing"), dir.path().to_path_buf()])
                .expect("scan should work");

        assert_eq!(snapshot.artifacts.len(), 2);
        assert_eq!(snapshot.total_bytes, 7);
    }

    #[test]
    fn merge_watch_events_with_scan_unions_paths() {
        let merged = merge_watch_events_with_scan(
            &["/tmp/a.json".into()].into_iter().collect(),
            &StorageSnapshot {
                artifacts: vec![ArtifactRecord {
                    path: "/tmp/b.json".into(),
                    bytes: 1,
                    modified_at: UNIX_EPOCH,
                }],
                total_bytes: 1,
            },
        );

        assert_eq!(merged.len(), 2);
        assert!(merged.contains(&PathBuf::from("/tmp/a.json")));
        assert!(merged.contains(&PathBuf::from("/tmp/b.json")));
    }

    #[test]
    fn stale_artifact_path_returns_only_entries_older_than_threshold() {
        let old = ArtifactRecord {
            path: "/tmp/old.json".into(),
            bytes: 1,
            modified_at: UNIX_EPOCH,
        };
        let fresh = ArtifactRecord {
            path: "/tmp/fresh.json".into(),
            bytes: 1,
            modified_at: UNIX_EPOCH + Duration::from_secs(95),
        };

        assert_eq!(
            stale_artifact_path(
                &old,
                UNIX_EPOCH + Duration::from_secs(100),
                Duration::from_secs(10)
            ),
            Some("/tmp/old.json".into())
        );
        assert_eq!(
            stale_artifact_path(
                &fresh,
                UNIX_EPOCH + Duration::from_secs(100),
                Duration::from_secs(10)
            ),
            None
        );
    }

    #[test]
    fn stale_artifacts_collects_all_stale_paths() {
        let snapshot = StorageSnapshot {
            artifacts: vec![
                ArtifactRecord {
                    path: "/tmp/old.json".into(),
                    bytes: 1,
                    modified_at: UNIX_EPOCH,
                },
                ArtifactRecord {
                    path: "/tmp/fresh.json".into(),
                    bytes: 1,
                    modified_at: UNIX_EPOCH + Duration::from_secs(95),
                },
            ],
            total_bytes: 2,
        };

        let stale = stale_artifacts(
            &snapshot,
            UNIX_EPOCH + Duration::from_secs(100),
            Duration::from_secs(10),
        );

        assert_eq!(stale, vec![PathBuf::from("/tmp/old.json")]);
    }
}
