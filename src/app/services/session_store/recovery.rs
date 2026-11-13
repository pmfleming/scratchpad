//! Explicit recovery copies never inherit the protected document's identity or path.
use super::SessionStore;
use crate::app::domain::{BufferState, WorkspaceTab};
use futures_lite::future::block_on;
use std::{
    io::{self, Write},
    path::{Path, PathBuf},
};

pub(crate) const RECOVERY_LIMIT: usize = 32 * 1024 * 1024;

#[derive(Clone, Debug)]
pub(crate) struct RecoveryItem {
    pub id: String,
    pub name: String,
    pub path: Option<PathBuf>,
    pub reason: String,
}

#[derive(Clone, Debug)]
pub(crate) struct RecoveryPreview {
    pub item: RecoveryItem,
    pub text: String,
    pub gaps: usize,
    pub truncated: bool,
}

impl RecoveryPreview {
    pub fn editable_copy(&self) -> WorkspaceTab {
        let mut buffer = BufferState::new(
            format!("{} (recovered)", self.item.name),
            self.text.clone(),
            None,
        );
        buffer.is_dirty = true;
        WorkspaceTab::new(buffer)
    }

    /// Exclusive creation also prevents overwriting a symlink or hard link to the original.
    pub fn save_copy(&self, path: &Path) -> io::Result<()> {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(path)?;
        file.write_all(self.text.as_bytes())?;
        file.sync_all()
    }

    pub fn disk_text(&self) -> io::Result<String> {
        let path = self
            .item
            .path
            .as_ref()
            .ok_or_else(|| io::Error::other("This document has no disk file."))?;
        if std::fs::metadata(path)?.len() > RECOVERY_LIMIT as u64 {
            return Err(io::Error::other(
                "Disk comparison is limited to 32 MiB. The original file is unchanged.",
            ));
        }
        let content = crate::app::services::file_service::FileService::read_file(path)?;
        if content.format.has_decoding_warnings {
            return Err(io::Error::other(
                "The disk file could not be decoded without substitutions. Choose its encoding in a separate tab.",
            ));
        }
        content.document.try_extract_text()
    }
}

impl SessionStore {
    pub(crate) fn recovery_items(&self) -> io::Result<Vec<RecoveryItem>> {
        self.worker()?.call(|db| block_on(db.recovery_items()))
    }
    pub(crate) fn recovery_preview(&self, id: String) -> io::Result<RecoveryPreview> {
        self.worker()?
            .call(move |db| block_on(db.recovery_preview(&id)))
    }
}

#[cfg(test)]
mod tests {
    use super::{RecoveryItem, RecoveryPreview};

    #[test]
    fn exports_are_exclusive_and_disk_comparison_preserves_both_versions() {
        let root = tempfile::tempdir().unwrap();
        let disk = root.path().join("original.txt");
        std::fs::write(&disk, "disk text").unwrap();
        let preview = RecoveryPreview {
            item: RecoveryItem {
                id: "protected-id".into(),
                name: "original.txt".into(),
                path: Some(disk.clone()),
                reason: "damaged history".into(),
            },
            text: "unsaved text\n[RECOVERY GAP: missing text]\n".into(),
            gaps: 1,
            truncated: false,
        };
        assert_eq!(preview.disk_text().unwrap(), "disk text");
        assert!(preview.save_copy(&disk).is_err());
        let copy = root.path().join("recovered.txt");
        preview.save_copy(&copy).unwrap();
        assert_eq!(std::fs::read_to_string(&copy).unwrap(), preview.text);
        assert_eq!(std::fs::read_to_string(&disk).unwrap(), "disk text");
        assert!(preview.save_copy(&copy).is_err());
        std::fs::remove_file(&disk).unwrap();
        assert!(preview.disk_text().is_err());
    }

    #[cfg(unix)]
    #[test]
    fn recovery_export_does_not_follow_an_existing_symlink() {
        let root = tempfile::tempdir().unwrap();
        let original = root.path().join("original.txt");
        let link = root.path().join("copy.txt");
        std::fs::write(&original, "original").unwrap();
        std::os::unix::fs::symlink(&original, &link).unwrap();
        let preview = RecoveryPreview {
            item: RecoveryItem {
                id: "id".into(),
                name: "note".into(),
                path: None,
                reason: "damage".into(),
            },
            text: "recovered".into(),
            gaps: 0,
            truncated: false,
        };
        assert!(preview.save_copy(&link).is_err());
        assert_eq!(std::fs::read_to_string(original).unwrap(), "original");
    }
}
