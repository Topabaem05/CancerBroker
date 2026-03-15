use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use notify::event::{CreateKind, ModifyKind, RenameMode};
use notify::{Event, EventKind};

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

fn find_artifact(snapshot: &StorageSnapshot, path: &Path) -> Result<usize, usize> {
    snapshot
        .artifacts
        .binary_search_by(|artifact| artifact.path.as_path().cmp(path))
}

fn refresh_totals(snapshot: &mut StorageSnapshot) {
    snapshot.total_bytes = snapshot.artifacts.iter().fold(0_u64, |total, artifact| {
        total.saturating_add(artifact.bytes)
    });
}

fn artifact_from_path(path: &Path) -> std::io::Result<Option<ArtifactRecord>> {
    let metadata = fs::metadata(path)?;
    if !metadata.is_file() {
        return Ok(None);
    }

    Ok(Some(ArtifactRecord {
        path: path.to_path_buf(),
        bytes: metadata.len(),
        modified_at: metadata.modified()?,
    }))
}

fn artifacts_from_path(path: &Path) -> std::io::Result<Vec<ArtifactRecord>> {
    let metadata = fs::metadata(path)?;
    if metadata.is_dir() {
        let mut artifacts = Vec::new();
        walk_files(path, &mut artifacts)?;
        return Ok(artifacts);
    }

    Ok(artifact_from_path(path)?.into_iter().collect())
}

pub fn upsert_artifact(snapshot: &mut StorageSnapshot, artifact: ArtifactRecord) -> bool {
    match find_artifact(snapshot, &artifact.path) {
        Ok(index) => {
            if snapshot.artifacts[index].bytes == artifact.bytes
                && snapshot.artifacts[index].modified_at == artifact.modified_at
            {
                return false;
            }

            snapshot.total_bytes = snapshot
                .total_bytes
                .saturating_sub(snapshot.artifacts[index].bytes)
                .saturating_add(artifact.bytes);
            snapshot.artifacts[index] = artifact;
            true
        }
        Err(index) => {
            snapshot.total_bytes = snapshot.total_bytes.saturating_add(artifact.bytes);
            snapshot.artifacts.insert(index, artifact);
            true
        }
    }
}

pub fn remove_artifacts_at_or_under(snapshot: &mut StorageSnapshot, path: &Path) -> bool {
    let original_len = snapshot.artifacts.len();
    snapshot
        .artifacts
        .retain(|artifact| !artifact.path.starts_with(path));

    if snapshot.artifacts.len() == original_len {
        return false;
    }

    refresh_totals(snapshot);
    true
}

fn refresh_paths(snapshot: &mut StorageSnapshot, paths: &[PathBuf]) -> bool {
    if paths.is_empty() {
        return false;
    }

    for path in paths {
        let Ok(artifacts) = artifacts_from_path(path) else {
            return false;
        };
        for artifact in artifacts {
            upsert_artifact(snapshot, artifact);
        }
    }

    true
}

fn apply_rename(snapshot: &mut StorageSnapshot, paths: &[PathBuf]) -> bool {
    if paths.len() != 2 {
        return false;
    }

    let from = &paths[0];
    let to = &paths[1];

    remove_artifacts_at_or_under(snapshot, from);
    refresh_paths(snapshot, std::slice::from_ref(to))
}

fn try_apply_watch_event(snapshot: &mut StorageSnapshot, event: &Event) -> bool {
    if event.need_rescan() {
        return false;
    }

    match event.kind {
        EventKind::Access(_) => true,
        EventKind::Create(CreateKind::File)
        | EventKind::Create(CreateKind::Folder)
        | EventKind::Create(CreateKind::Any)
        | EventKind::Create(CreateKind::Other)
        | EventKind::Modify(ModifyKind::Data(_))
        | EventKind::Modify(ModifyKind::Metadata(_)) => refresh_paths(snapshot, &event.paths),
        EventKind::Modify(ModifyKind::Name(RenameMode::Both)) => {
            apply_rename(snapshot, &event.paths)
        }
        EventKind::Remove(_) => {
            !event.paths.is_empty()
                && event.paths.iter().fold(false, |changed, path| {
                    changed | remove_artifacts_at_or_under(snapshot, path)
                })
        }
        _ => false,
    }
}

pub fn try_apply_watch_events_incremental(
    snapshot: &mut StorageSnapshot,
    events: &[Event],
) -> bool {
    if events.is_empty() {
        return true;
    }

    for event in events {
        if !try_apply_watch_event(snapshot, event) {
            return false;
        }
    }

    true
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

    use notify::event::{CreateKind, DataChange, ModifyKind, RemoveKind, RenameMode};
    use notify::{Event, EventKind};
    use tempfile::tempdir;

    use super::{
        ArtifactRecord, StorageSnapshot, finalize_snapshot, merge_watch_events_with_scan,
        scan_allowlisted_roots, stale_artifact_path, stale_artifacts,
        try_apply_watch_events_incremental, upsert_artifact,
    };

    fn watch_event(kind: EventKind, paths: Vec<PathBuf>) -> Event {
        Event {
            kind,
            paths,
            attrs: Default::default(),
        }
    }

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

    #[test]
    fn upsert_artifact_keeps_snapshot_sorted() {
        let mut snapshot = StorageSnapshot::default();

        upsert_artifact(
            &mut snapshot,
            ArtifactRecord {
                path: "/tmp/b.json".into(),
                bytes: 2,
                modified_at: UNIX_EPOCH,
            },
        );
        upsert_artifact(
            &mut snapshot,
            ArtifactRecord {
                path: "/tmp/a.json".into(),
                bytes: 1,
                modified_at: UNIX_EPOCH,
            },
        );

        assert_eq!(snapshot.artifacts[0].path, PathBuf::from("/tmp/a.json"));
        assert_eq!(snapshot.artifacts[1].path, PathBuf::from("/tmp/b.json"));
        assert_eq!(snapshot.total_bytes, 3);
    }

    #[test]
    fn try_apply_watch_events_incremental_tracks_file_lifecycle() {
        let dir = tempdir().expect("tempdir");
        let artifact = dir.path().join("a.json");
        let mut snapshot = StorageSnapshot::default();

        fs::write(&artifact, "abc").expect("artifact should be written");
        assert!(try_apply_watch_events_incremental(
            &mut snapshot,
            &[watch_event(
                EventKind::Create(CreateKind::File),
                vec![artifact.clone()],
            )],
        ));
        assert_eq!(snapshot.total_bytes, 3);

        fs::write(&artifact, "abcdef").expect("artifact should be updated");
        assert!(try_apply_watch_events_incremental(
            &mut snapshot,
            &[watch_event(
                EventKind::Modify(ModifyKind::Data(DataChange::Content)),
                vec![artifact.clone()],
            )],
        ));
        assert_eq!(snapshot.total_bytes, 6);

        fs::remove_file(&artifact).expect("artifact should be removed");
        assert!(try_apply_watch_events_incremental(
            &mut snapshot,
            &[watch_event(
                EventKind::Remove(RemoveKind::File),
                vec![artifact],
            )],
        ));
        assert!(snapshot.artifacts.is_empty());
        assert_eq!(snapshot.total_bytes, 0);
    }

    #[test]
    fn try_apply_watch_events_incremental_renames_file() {
        let dir = tempdir().expect("tempdir");
        let old_path = dir.path().join("old.json");
        let new_path = dir.path().join("new.json");
        fs::write(&old_path, "abc").expect("old artifact should exist");

        let mut snapshot = scan_allowlisted_roots(&[dir.path().to_path_buf()]).expect("scan");
        fs::rename(&old_path, &new_path).expect("rename should succeed");

        assert!(try_apply_watch_events_incremental(
            &mut snapshot,
            &[watch_event(
                EventKind::Modify(ModifyKind::Name(RenameMode::Both)),
                vec![old_path, new_path.clone()],
            )],
        ));
        assert_eq!(snapshot.artifacts.len(), 1);
        assert_eq!(snapshot.artifacts[0].path, new_path);
    }

    #[test]
    fn try_apply_watch_events_incremental_renames_directory_subtree() {
        let dir = tempdir().expect("tempdir");
        let nested = dir.path().join("nested");
        let renamed = dir.path().join("renamed");
        fs::create_dir_all(&nested).expect("nested dir");
        fs::write(nested.join("artifact.json"), "abc").expect("artifact should exist");

        let mut snapshot = scan_allowlisted_roots(&[dir.path().to_path_buf()]).expect("scan");
        fs::rename(&nested, &renamed).expect("rename should succeed");

        assert!(try_apply_watch_events_incremental(
            &mut snapshot,
            &[watch_event(
                EventKind::Modify(ModifyKind::Name(RenameMode::Both)),
                vec![nested, renamed.clone()],
            )],
        ));
        assert_eq!(snapshot.artifacts.len(), 1);
        assert_eq!(snapshot.artifacts[0].path, renamed.join("artifact.json"));
    }
}
