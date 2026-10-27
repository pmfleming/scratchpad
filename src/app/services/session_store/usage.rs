use super::SessionStore;
use futures_lite::future::block_on;
use std::{fs, io, path::Path};

pub use super::database::usage::StoredUsage;

#[derive(Clone, Debug, Default)]
pub struct StorageUsage {
    pub database: u64,
    pub journal: u64,
    pub other: u64,
    pub settings: u64,
    pub total_file_bytes: u64,
    /// Filesystem allocated bytes on Unix, unavailable elsewhere.
    pub allocated_bytes: Option<u64>,
    pub stored: StoredUsage,
    pub stored_error: Option<String>,
}

impl SessionStore {
    /// Run off the UI thread. Does not checkpoint or modify the database.
    pub fn storage_usage(&self) -> io::Result<StorageUsage> {
        self.storage_usage_with_settings(self.root())
    }

    pub fn storage_usage_with_settings(&self, settings_root: &Path) -> io::Result<StorageUsage> {
        let result = self
            .worker()
            .and_then(|worker| worker.call(|db| block_on(db.stored_usage())));
        let mut usage = match result {
            Ok(stored) => StorageUsage {
                stored,
                ..StorageUsage::default()
            },
            Err(error) => StorageUsage {
                stored_error: Some(error.to_string()),
                ..StorageUsage::default()
            },
        };
        scan_directory_excluding(self.root(), self.root(), None, &mut usage)?;
        // Settings and state normally live in separate XDG/AppData directories.
        // Avoid counting the same files twice in portable/same-root setups.
        let root = self.root().canonicalize()?;
        match settings_root.canonicalize() {
            Ok(settings) if !settings.starts_with(&root) => {
                let mut extra = StorageUsage::default();
                scan_directory_excluding(&settings, &settings, Some(&root), &mut extra)?;
                usage.settings = extra.total_file_bytes;
                usage.total_file_bytes += extra.total_file_bytes;
                if let (Some(total), Some(bytes)) =
                    (&mut usage.allocated_bytes, extra.allocated_bytes)
                {
                    *total += bytes;
                }
            }
            Err(error) if error.kind() != io::ErrorKind::NotFound => return Err(error),
            _ => {}
        }
        Ok(usage)
    }
}

fn scan_directory_excluding(
    root: &Path,
    directory: &Path,
    exclude: Option<&Path>,
    usage: &mut StorageUsage,
) -> io::Result<()> {
    #[cfg(unix)]
    usage.allocated_bytes.get_or_insert(0);
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let metadata = match fs::symlink_metadata(entry.path()) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error),
        };
        if metadata.is_symlink() || exclude.is_some_and(|excluded| entry.path() == excluded) {
            continue;
        }
        if metadata.is_dir() {
            scan_directory_excluding(root, &entry.path(), exclude, usage)?;
        } else if metadata.is_file() {
            usage.record_file(entry.path().strip_prefix(root).ok(), &metadata);
        }
    }
    Ok(())
}

impl StorageUsage {
    fn record_file(&mut self, relative_path: Option<&Path>, metadata: &fs::Metadata) {
        let bytes = metadata.len();
        match relative_path.and_then(Path::to_str) {
            Some("session.db") => self.database += bytes,
            Some("session.db-wal" | "session.db-shm") => self.journal += bytes,
            _ => self.other += bytes,
        }
        self.total_file_bytes += bytes;
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            *self.allocated_bytes.get_or_insert(0) += metadata.blocks() * 512;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::SessionStore;
    use crate::app::domain::{BufferState, WorkspaceTab};
    use std::fs;

    #[test]
    fn usage_counts_payloads_and_roots_once_without_following_symlinks() {
        let root = tempfile::tempdir().unwrap();
        let store = SessionStore::new(root.path().join("state"));
        store
            .persist(
                &[WorkspaceTab::new(BufferState::new(
                    "usage".into(),
                    "recovery payload".repeat(100),
                    None,
                ))],
                0,
                14.0,
                false,
            )
            .unwrap();
        fs::write(store.root().join("other"), [0; 123]).unwrap();
        let usage = store.storage_usage().unwrap();
        assert!(usage.database > 0);
        assert!(usage.other >= 123);
        assert_eq!(
            usage.total_file_bytes,
            usage.database + usage.journal + usage.other
        );
        assert_eq!(usage.stored.shared_text, 1600);
        assert_eq!(usage.stored.live_text + usage.stored.recovery_text, 0);
        let settings = root.path().join("settings");
        fs::create_dir(&settings).unwrap();
        fs::write(settings.join("settings.toml"), [0; 321]).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&settings, store.root().join("outside")).unwrap();
        for (path, additional) in [
            (settings.as_path(), 321),
            (root.path(), 321),
            (store.root(), 0),
            (root.path().join("missing").as_path(), 0),
        ] {
            let combined = store.storage_usage_with_settings(path).unwrap();
            assert_eq!(combined.settings, additional, "{}", path.display());
            assert_eq!(
                combined.total_file_bytes,
                usage.total_file_bytes + additional
            );
        }
        assert!(
            store
                .storage_usage_with_settings(&settings.join("settings.toml"))
                .is_err()
        );
    }
}
