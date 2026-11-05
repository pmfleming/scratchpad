use super::{FileContent, FileService};
use crate::app::domain::{BufferState, DiskFileState};
use std::{io, path::Path};

pub(crate) struct ObservedFile {
    pub content: FileContent,
    pub state: DiskFileState,
    pub digest: String,
}

impl FileService {
    /// Clean replacements carry owned text to the UI. A file-backed document
    /// can evict hashed chunks, so extracting it later could otherwise read disk
    /// on the paint thread or load a version different from the observed digest.
    pub(crate) fn read_observed_buffer(
        path: &Path,
        materialize: bool,
        known_digest: Option<&str>,
    ) -> io::Result<BufferState> {
        for _ in 0..3 {
            let observed = Self::read_observed_file(path)?;
            let text = (materialize && Some(observed.digest.as_str()) != known_digest)
                .then(|| observed.content.document.extract_text());
            if let Some(text) = &text
                && (blake3::hash(text.as_bytes()).to_hex().as_str() != observed.digest
                    || Self::read_disk_state(path)? != observed.state)
            {
                continue;
            }
            let mut buffer = Self::buffer_from_observed(path, observed);
            buffer.loaded_replacement_text = text.map(std::sync::Arc::new);
            return Ok(buffer);
        }
        Err(io::Error::new(
            io::ErrorKind::WouldBlock,
            "File kept changing while preparing its replacement. Current text was kept.",
        ))
    }
    /// Run on an I/O worker. Hash decoded text before accepting its metadata;
    /// retry replacements/writes that occur while the file is being read.
    pub(crate) fn read_observed_file(path: &Path) -> io::Result<ObservedFile> {
        for _ in 0..3 {
            let before = Self::read_disk_state(path)?;
            let content = Self::read_file(path)?;
            let digest = content.document.content_digest();
            let after = Self::read_disk_state(path)?;
            if before == after {
                return Ok(ObservedFile {
                    content,
                    state: after,
                    digest,
                });
            }
        }
        Err(io::Error::new(
            io::ErrorKind::WouldBlock,
            "File kept changing while it was being read. Current text was kept.",
        ))
    }

    pub(crate) fn buffer_from_observed(path: &Path, observed: ObservedFile) -> BufferState {
        Self::build_buffer_with_digest(
            path,
            observed.content,
            Some(observed.state),
            Some(observed.digest),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn replacement_text_survives_source_changes_beyond_the_file_chunk_cache() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("large.txt");
        let text = "a".repeat(9 * 1024 * 1024);
        std::fs::write(&path, &text).unwrap();
        let loaded = FileService::read_observed_buffer(&path, true, None).unwrap();
        assert!(loaded.loaded_replacement_text.is_some());
        std::fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .unwrap()
            .write_all(b"z")
            .unwrap();
        let mut original = BufferState::new("original".into(), String::new(), Some(path));
        original.replace_from_loaded_buffer_source(
            loaded,
            crate::app::domain::buffer::PieceSource::External,
        );
        assert_eq!(original.text(), text);
        assert!(original.loaded_replacement_text.is_none());
        original.document_mut().undo_last_operation().unwrap();
        assert_eq!(original.text(), "");
    }

    #[test]
    fn dirty_and_content_identical_observations_do_not_allocate_replacement_text() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("note.txt");
        std::fs::write(&path, "saved").unwrap();
        let dirty = FileService::read_observed_buffer(&path, false, None).unwrap();
        assert!(dirty.loaded_replacement_text.is_none());
        let touch =
            FileService::read_observed_buffer(&path, true, dirty.observed_disk_digest.as_deref())
                .unwrap();
        assert!(touch.loaded_replacement_text.is_none());
        assert_eq!(touch.observed_disk_digest, dirty.observed_disk_digest);
    }
}
