use serde::{Deserialize, Serialize};

const MIB: u64 = 1024 * 1024;

/// Payload budgets, not a hard cap on SQLite's database/WAL allocation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct RecoverySettings {
    pub snapshot_byte_budget: u64,
    pub archive_byte_budget: u64,
    /// Zero disables age-based expiry (the storage budget still applies).
    pub archive_retention_days: u32,
    pub reopen_archives: bool,
    /// Protected archives may exceed the archive budget; they are never silently deleted.
    pub pinned_archives: Vec<String>,
}

impl Default for RecoverySettings {
    fn default() -> Self {
        Self {
            snapshot_byte_budget: 256 * MIB,
            archive_byte_budget: 512 * MIB,
            archive_retention_days: 90,
            reopen_archives: true,
            pinned_archives: Vec::new(),
        }
    }
}

impl RecoverySettings {
    pub fn sanitized(mut self) -> Self {
        self.snapshot_byte_budget = self.snapshot_byte_budget.clamp(MIB, 16 * 1024 * MIB);
        self.archive_byte_budget = self.archive_byte_budget.clamp(MIB, 64 * 1024 * MIB);
        self.archive_retention_days = self.archive_retention_days.min(36500);
        self.pinned_archives.sort();
        self.pinned_archives.dedup();
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_settings_gain_recovery_defaults_and_round_trip() {
        let settings: super::super::AppSettings = toml::from_str("").unwrap();
        assert_eq!(settings.history.recovery, RecoverySettings::default());
        let serialized = toml::to_string(&settings).unwrap();
        let restored: super::super::AppSettings = toml::from_str(&serialized).unwrap();
        assert_eq!(settings, restored);
    }
}
