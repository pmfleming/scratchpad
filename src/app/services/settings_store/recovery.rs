use serde::{Deserialize, Serialize};

/// Reopening preferences. History has no automatic expiry or payload limit.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct RecoverySettings {
    pub reopen_archives: bool,
    pub open_changed_disk_files: bool,
}

impl Default for RecoverySettings {
    fn default() -> Self {
        Self {
            reopen_archives: true,
            open_changed_disk_files: true,
        }
    }
}

impl RecoverySettings {
    #[must_use]
    pub fn sanitized(self) -> Self {
        self
    }
}
