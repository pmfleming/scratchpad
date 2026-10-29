use serde::{Deserialize, Deserializer, Serialize};
use std::collections::BTreeMap;

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
    pub open_changed_disk_files: bool,
    /// One-time, 30-day migration grace for archives pinned by older versions.
    /// Not a user-facing pinning feature; deadlines survive settings reloads.
    #[serde(
        alias = "pinned_archives",
        deserialize_with = "deserialize_archive_grace",
        skip_serializing_if = "BTreeMap::is_empty"
    )]
    pub archive_migration_grace_until: BTreeMap<String, i64>,
}

impl Default for RecoverySettings {
    fn default() -> Self {
        Self {
            snapshot_byte_budget: 256 * MIB,
            archive_byte_budget: 512 * MIB,
            archive_retention_days: 90,
            reopen_archives: true,
            open_changed_disk_files: true,
            archive_migration_grace_until: BTreeMap::new(),
        }
    }
}

fn now_seconds() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .min(i64::MAX as u64) as i64
}

fn deserialize_archive_grace<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<BTreeMap<String, i64>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Stored {
        Deadlines(BTreeMap<String, i64>),
        LegacyPins(Vec<String>),
    }
    Ok(match Stored::deserialize(deserializer)? {
        Stored::Deadlines(deadlines) => deadlines,
        Stored::LegacyPins(tokens) => {
            let until = now_seconds().saturating_add(30 * 86400);
            tokens.into_iter().map(|token| (token, until)).collect()
        }
    })
}

impl RecoverySettings {
    pub(crate) fn archive_has_migration_grace(&self, token: &str) -> bool {
        self.archive_migration_grace_until
            .get(token)
            .is_some_and(|until| *until > now_seconds())
    }

    #[must_use]
    pub fn sanitized(mut self) -> Self {
        self.snapshot_byte_budget = self.snapshot_byte_budget.clamp(MIB, 16 * 1024 * MIB);
        self.archive_byte_budget = self.archive_byte_budget.clamp(MIB, 64 * 1024 * MIB);
        self.archive_retention_days = self.archive_retention_days.min(36500);
        let now = now_seconds();
        self.archive_migration_grace_until
            .retain(|_, until| *until > now);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_pins_migrate_once_to_finite_persisted_grace() {
        let root = tempfile::tempdir().unwrap();
        let store = super::super::SettingsStore::new(root.path().to_owned());
        let raw = toml::to_string(&super::super::AppSettings::default())
            .unwrap()
            .replace(
                "[history.recovery]",
                "[history.recovery]\npinned_archives = ['saved', 'saved']",
            );
        std::fs::write(store.path(), raw).unwrap();
        let before = now_seconds();
        let settings = store.load().unwrap().unwrap();
        let policy = &settings.history.recovery;
        assert_eq!(policy.archive_migration_grace_until.len(), 1);
        let deadline = policy.archive_migration_grace_until["saved"];
        assert!(deadline >= before + 30 * 86400);
        assert!(deadline <= now_seconds() + 30 * 86400);
        assert!(policy.archive_has_migration_grace("saved"));
        let raw = std::fs::read_to_string(store.path()).unwrap();
        assert!(!raw.contains("pinned_archives"));
        assert!(raw.contains("archive_migration_grace_until"));
        assert_eq!(settings, store.load().unwrap().unwrap());
        let expired = RecoverySettings {
            archive_migration_grace_until: [("saved".into(), 0)].into(),
            ..Default::default()
        };
        assert!(!expired.archive_has_migration_grace("saved"));
        assert!(expired.sanitized().archive_migration_grace_until.is_empty());
    }

    #[test]
    fn old_settings_gain_recovery_defaults_and_round_trip() {
        let settings: super::super::AppSettings = toml::from_str("").unwrap();
        assert_eq!(settings.history.recovery, RecoverySettings::default());
        let serialized = toml::to_string(&settings).unwrap();
        let restored: super::super::AppSettings = toml::from_str(&serialized).unwrap();
        assert_eq!(settings, restored);
    }
}
