use super::SessionStore;
use futures_lite::future::block_on;
use std::{fs, io, path::Path};

pub use super::database::usage::StoredUsage;

#[derive(Clone, Debug, Default)]
pub struct StorageUsage {
    pub database: u64,
    pub journal: u64,
    pub other: u64,
    pub total_file_bytes: u64,
    /// Filesystem allocated bytes on Unix, unavailable elsewhere.
    pub allocated_bytes: Option<u64>,
    pub stored: StoredUsage,
}

impl SessionStore {
    /// Run off the UI thread. Does not checkpoint or modify the database.
    pub fn storage_usage(&self) -> io::Result<StorageUsage> {
        let stored = self.worker()?.call(|db| block_on(db.stored_usage()))?;
        let mut usage = StorageUsage { stored, ..StorageUsage::default() };
        #[cfg(unix)]
        { usage.allocated_bytes = Some(0); }
        scan_directory(self.root(), self.root(), &mut usage)?;
        Ok(usage)
    }
}

fn scan_directory(root: &Path, directory: &Path, usage: &mut StorageUsage) -> io::Result<()> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let metadata = fs::symlink_metadata(entry.path())?;
        if metadata.is_symlink() { continue; }
        if metadata.is_dir() {
            scan_directory(root, &entry.path(), usage)?;
        } else if metadata.is_file() {
            let bytes = metadata.len();
            match entry.path().strip_prefix(root).ok().and_then(Path::to_str) {
                Some("session.db") => usage.database += bytes,
                Some("session.db-wal" | "session.db-shm") => usage.journal += bytes,
                _ => usage.other += bytes,
            }
            usage.total_file_bytes += bytes;
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;
                *usage.allocated_bytes.as_mut().unwrap() += metadata.blocks() * 512;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reports_real_files_and_stored_payloads() {
        let root = tempfile::tempdir().unwrap();
        let store = SessionStore::new(root.path().to_owned());
        let tab = crate::app::domain::WorkspaceTab::new(crate::app::domain::BufferState::new(
            "usage".into(), "recovery payload".repeat(100), None,
        ));
        store.persist(&[tab], 0, 14.0, false).unwrap();
        fs::write(root.path().join("other"), [0; 123]).unwrap();
        let usage = store.storage_usage().unwrap();
        assert!(usage.database > 0);
        assert!(usage.other >= 123);
        assert_eq!(usage.total_file_bytes, usage.database + usage.journal + usage.other);
        assert_eq!(usage.stored.shared_text, 1600);
        assert_eq!(usage.stored.live_text + usage.stored.recovery_text, 0);
    }
}
