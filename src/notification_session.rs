use std::fs::{self, File};
use std::io::BufWriter;
use std::path::Path;
use std::time::SystemTime;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::platform::current_effective_uid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotificationSessionSnapshot {
    pub captured_at_unix_secs: u64,
    pub uid: u32,
    pub dbus_session_bus_address: Option<String>,
    pub xdg_runtime_dir: Option<String>,
    pub display: Option<String>,
    pub wayland_display: Option<String>,
    pub xdg_session_type: Option<String>,
}

#[derive(Debug, Error)]
pub enum NotificationSessionError {
    #[error("notification session io error at {path}: {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },
    #[error("notification session parse error at {path}: {source}")]
    Parse {
        path: String,
        source: serde_json::Error,
    },
    #[error("notification session serialize error: {0}")]
    Serialize(serde_json::Error),
}

impl NotificationSessionSnapshot {
    fn capture() -> Self {
        Self {
            captured_at_unix_secs: unix_timestamp_secs(SystemTime::now()),
            uid: current_effective_uid(),
            dbus_session_bus_address: env_var("DBUS_SESSION_BUS_ADDRESS"),
            xdg_runtime_dir: env_var("XDG_RUNTIME_DIR"),
            display: env_var("DISPLAY"),
            wayland_display: env_var("WAYLAND_DISPLAY"),
            xdg_session_type: env_var("XDG_SESSION_TYPE"),
        }
    }

    pub fn is_usable_for_current_process(&self) -> bool {
        if self.uid != current_effective_uid() {
            return false;
        }

        #[cfg(target_os = "linux")]
        {
            self.dbus_session_bus_address.is_some()
                && self.xdg_runtime_dir.is_some()
                && (self.display.is_some() || self.wayland_display.is_some())
        }

        #[cfg(not(target_os = "linux"))]
        {
            false
        }
    }

    pub fn env_pairs(&self) -> Vec<(&'static str, &str)> {
        let mut pairs = Vec::new();
        if let Some(value) = self.dbus_session_bus_address.as_deref() {
            pairs.push(("DBUS_SESSION_BUS_ADDRESS", value));
        }
        if let Some(value) = self.xdg_runtime_dir.as_deref() {
            pairs.push(("XDG_RUNTIME_DIR", value));
        }
        if let Some(value) = self.display.as_deref() {
            pairs.push(("DISPLAY", value));
        }
        if let Some(value) = self.wayland_display.as_deref() {
            pairs.push(("WAYLAND_DISPLAY", value));
        }
        if let Some(value) = self.xdg_session_type.as_deref() {
            pairs.push(("XDG_SESSION_TYPE", value));
        }
        pairs
    }
}

pub fn refresh_notification_session_snapshot(
    path: &Path,
) -> Result<bool, NotificationSessionError> {
    let snapshot = NotificationSessionSnapshot::capture();
    if !snapshot.is_usable_for_current_process() {
        return Ok(false);
    }

    persist_snapshot(path, &snapshot)?;
    Ok(true)
}

pub fn load_notification_session_snapshot(
    path: &Path,
) -> Result<Option<NotificationSessionSnapshot>, NotificationSessionError> {
    if !path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(path).map_err(|source| NotificationSessionError::Io {
        path: path.display().to_string(),
        source,
    })?;

    let snapshot =
        serde_json::from_str::<NotificationSessionSnapshot>(&content).map_err(|source| {
            NotificationSessionError::Parse {
                path: path.display().to_string(),
                source,
            }
        })?;

    if snapshot.is_usable_for_current_process() {
        Ok(Some(snapshot))
    } else {
        Ok(None)
    }
}

fn persist_snapshot(
    path: &Path,
    snapshot: &NotificationSessionSnapshot,
) -> Result<(), NotificationSessionError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| NotificationSessionError::Io {
            path: parent.display().to_string(),
            source,
        })?;
    }

    let file = File::create(path).map_err(|source| NotificationSessionError::Io {
        path: path.display().to_string(),
        source,
    })?;
    let writer = BufWriter::new(file);

    serde_json::to_writer_pretty(writer, snapshot).map_err(NotificationSessionError::Serialize)
}

fn env_var(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|value| !value.is_empty())
}

fn unix_timestamp_secs(now: SystemTime) -> u64 {
    now.duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::{load_notification_session_snapshot, refresh_notification_session_snapshot};

    #[cfg(target_os = "linux")]
    use super::NotificationSessionSnapshot;

    #[test]
    fn load_notification_session_snapshot_returns_none_for_missing_file() {
        let temp = tempdir().expect("tempdir");
        let path = temp.path().join("notify-session.json");
        assert!(
            load_notification_session_snapshot(&path)
                .expect("missing path should load")
                .is_none()
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn snapshot_validation_requires_bus_runtime_and_display() {
        let snapshot = NotificationSessionSnapshot {
            captured_at_unix_secs: 1,
            uid: crate::platform::current_effective_uid(),
            dbus_session_bus_address: Some("unix:path=/tmp/dbus-test".to_string()),
            xdg_runtime_dir: Some("/run/user/501".to_string()),
            display: Some(":0".to_string()),
            wayland_display: None,
            xdg_session_type: Some("x11".to_string()),
        };
        assert!(snapshot.is_usable_for_current_process());
    }

    #[test]
    fn refresh_notification_session_snapshot_is_nonfatal_without_desktop_session() {
        let temp = tempdir().expect("tempdir");
        let path = temp.path().join("notify-session.json");
        let refreshed = refresh_notification_session_snapshot(&path)
            .expect("refresh should not fail without desktop session");

        #[cfg(target_os = "linux")]
        assert!(!refreshed);
        #[cfg(not(target_os = "linux"))]
        assert!(!refreshed);
    }
}
