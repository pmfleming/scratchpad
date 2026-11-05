use super::{FileContent, FileService};
use crate::app::domain::{BufferState, DiskFileState};
use std::{
    io::{self, Read},
    path::Path,
};

pub(crate) struct ObservedFile {
    pub content: FileContent,
    pub state: DiskFileState,
    pub digest: String,
}

impl FileService {
    /// Clean replacements carry owned text to the UI. A file-backed document
    /// can evict hashed chunks, so extracting it later could otherwise read disk
    /// on the paint thread or load a version different from the observed digest.
    #[cfg(test)]
    pub(crate) fn read_observed_buffer(
        path: &Path,
        materialize: bool,
        known_digest: Option<&str>,
    ) -> io::Result<BufferState> {
        Self::read_observed_buffer_with_context(
            path,
            materialize,
            known_digest,
            &super::super::chunk_store::ImportContext::default(),
        )
    }
    pub(crate) fn read_observed_buffer_with_context(
        path: &Path,
        materialize: bool,
        known_digest: Option<&str>,
        context: &super::super::chunk_store::ImportContext,
    ) -> io::Result<BufferState> {
        context.check()?;
        let (state, digest) = Self::observe_decoded_digest_with_context(path, context)?;
        if !materialize || known_digest == Some(digest.as_str()) {
            // Transport only: callers apply its observation metadata and never
            // adopt its empty document. No original/add chunks are imported.
            let mut buffer = Self::build_cold_file_shell(path, Some(state));
            buffer.disk_content_digest = Some(digest.clone());
            buffer.observed_disk_digest = Some(digest);
            return Ok(buffer);
        }
        let observed = Self::read_observed_file_with_context(path, context)?;
        let mut buffer = Self::buffer_from_observed(path, observed);
        buffer.replacement_ready = true;
        Ok(buffer)
    }

    /// Hash the same decoded UTF-8 as adoption while retaining bounded buffers.
    /// Decoder state preserves split code points/BOMs and CRLF across reads.
    #[cfg(test)]
    pub(crate) fn observe_decoded_digest(path: &Path) -> io::Result<(DiskFileState, String)> {
        Self::observe_decoded_digest_with_context(
            path,
            &super::super::chunk_store::ImportContext::default(),
        )
    }
    pub(crate) fn observe_decoded_digest_with_context(
        path: &Path,
        context: &super::super::chunk_store::ImportContext,
    ) -> io::Result<(DiskFileState, String)> {
        for _ in 0..3 {
            let before = Self::read_disk_state(path)?;
            let prefix = super::read::inspect_file_prefix(path)?;
            let mut decoder = if prefix.has_bom {
                prefix.encoding.new_decoder_with_bom_removal()
            } else {
                prefix.encoding.new_decoder_without_bom_handling()
            };
            let mut file = std::fs::File::open(path)?;
            let mut raw = [0_u8; 16 * 1024];
            let mut decoded = [0_u8; 32 * 1024];
            let mut hasher = blake3::Hasher::new();
            loop {
                context.check()?;
                let read = file.read(&mut raw)?;
                let last = read == 0;
                let mut consumed = 0;
                loop {
                    let (result, taken, written, _warnings) =
                        decoder.decode_to_utf8(&raw[consumed..read], &mut decoded, last);
                    consumed += taken;
                    if decoded[..written].contains(&0) {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "Binary files are not supported",
                        ));
                    }
                    hasher.update(&decoded[..written]);
                    if result == encoding_rs::CoderResult::InputEmpty {
                        break;
                    }
                }
                if last {
                    break;
                }
            }
            let after = Self::read_disk_state(path)?;
            if before == after {
                return Ok((after, hasher.finalize().to_hex().to_string()));
            }
        }
        Err(io::Error::new(
            io::ErrorKind::WouldBlock,
            "File kept changing during observation. Current text was kept.",
        ))
    }
    /// Run on an I/O worker. Hash decoded text before accepting its metadata;
    /// retry replacements/writes that occur while the file is being read.
    #[cfg(test)]
    pub(crate) fn read_observed_file(path: &Path) -> io::Result<ObservedFile> {
        Self::read_observed_file_with_context(
            path,
            &super::super::chunk_store::ImportContext::default(),
        )
    }
    pub(crate) fn read_observed_file_with_context(
        path: &Path,
        context: &super::super::chunk_store::ImportContext,
    ) -> io::Result<ObservedFile> {
        for _ in 0..3 {
            let before = Self::read_disk_state(path)?;
            let content = Self::read_file_with_context(path, context)?;
            let digest = content
                .decoded_digest
                .clone()
                .map_or_else(|| content.document.try_content_digest(), Ok)?;
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
        let text = "a".repeat(17 * 1024 * 1024);
        std::fs::write(&path, &text).unwrap();
        let loaded = FileService::read_observed_buffer(&path, true, None).unwrap();
        assert!(loaded.replacement_ready);
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
        assert!(!original.replacement_ready);
        original.document_mut().undo_last_operation().unwrap();
        assert_eq!(original.text(), "");
    }

    #[test]
    fn dirty_and_content_identical_observations_do_not_allocate_replacement_text() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("note.txt");
        std::fs::write(&path, "saved").unwrap();
        let dirty = FileService::read_observed_buffer(&path, false, None).unwrap();
        assert!(!dirty.replacement_ready);
        let touch =
            FileService::read_observed_buffer(&path, true, dirty.observed_disk_digest.as_deref())
                .unwrap();
        assert!(!touch.replacement_ready);
        assert_eq!(touch.observed_disk_digest, dirty.observed_disk_digest);
        assert_eq!(dirty.document().piece_tree().len_bytes(), 0);
        assert!(dirty.document().history_entries().is_empty());
    }

    #[test]
    fn unchanged_large_observation_writes_no_chunks_and_cancelled_import_is_atomic() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("large");
        std::fs::write(&path, "λ🦀\r\n".repeat(2_200_000)).unwrap();
        let session =
            crate::app::services::session_store::SessionStore::new(root.path().join("session"));
        let store = session.chunk_store_handle().unwrap();
        let context = crate::app::services::chunk_store::ImportContext {
            store: Some(store.clone()),
            ..Default::default()
        };
        let initial = FileService::read_observed_file_with_context(&path, &context).unwrap();
        session.flush_text_chunks().unwrap();
        let hashed = store.hashed_bytes();
        let unchanged = FileService::read_observed_buffer_with_context(
            &path,
            true,
            Some(&initial.digest),
            &context,
        )
        .unwrap();
        assert!(unchanged.document().history_entries().is_empty());
        session.flush_text_chunks().unwrap();
        assert_eq!(store.hashed_bytes(), hashed);
        let cancelled = crate::app::services::chunk_store::ImportContext {
            cancelled: Some(std::sync::Arc::new(std::sync::atomic::AtomicBool::new(
                true,
            ))),
            ..context
        };
        assert_eq!(
            FileService::read_file_with_context(&path, &cancelled)
                .err()
                .unwrap()
                .kind(),
            io::ErrorKind::Interrupted
        );
        assert_eq!(store.pending_bytes(), 0);
        assert_eq!(
            initial.content.document.try_content_digest().unwrap(),
            initial.digest
        );
    }

    #[test]
    fn observation_matches_adoption_across_decoder_boundaries_and_boms() {
        let directory = tempfile::tempdir().unwrap();
        let text = "café 東京 λ\r\n".repeat(8192);
        for (name, bytes) in [
            ("utf8", text.as_bytes().to_vec()),
            (
                "utf8-bom",
                [b"\xef\xbb\xbf".as_slice(), text.as_bytes()].concat(),
            ),
            (
                "utf16le",
                [
                    b"\xff\xfe".to_vec(),
                    text.encode_utf16().flat_map(u16::to_le_bytes).collect(),
                ]
                .concat(),
            ),
            (
                "utf16be",
                [
                    b"\xfe\xff".to_vec(),
                    text.encode_utf16().flat_map(u16::to_be_bytes).collect(),
                ]
                .concat(),
            ),
            ("lossy-utf8", [text.as_bytes(), b"\xe2\x82"].concat()),
        ] {
            let path = directory.path().join(name);
            std::fs::write(&path, bytes).unwrap();
            let adopted = FileService::read_file(&path).unwrap();
            let (_, digest) = FileService::observe_decoded_digest(&path).unwrap();
            assert_eq!(
                digest,
                adopted.document.try_content_digest().unwrap(),
                "{name}"
            );
        }
    }
}

#[cfg(test)]
mod reference_adoption_tests {
    use super::*;
    use crate::app::domain::{PieceSource, WorkspaceTab};
    use crate::app::services::{chunk_store::ImportContext, session_store::SessionStore};
    #[test]
    fn large_changed_source_adopts_references_without_body_reads_or_rewriting_imported_chunks() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("source");
        let original = "αβ\n".repeat(3_500_000);
        std::fs::write(&path, &original).unwrap();
        let session = SessionStore::new(root.path().join("session"));
        let store = session.chunk_store_handle().unwrap();
        let context = ImportContext {
            store: Some(store.clone()),
            ..Default::default()
        };
        let observed = FileService::read_observed_file_with_context(&path, &context).unwrap();
        let mut buffer = FileService::buffer_from_observed(&path, observed);
        session
            .persist(
                std::slice::from_ref(&WorkspaceTab::new(buffer.clone())),
                0,
                14.,
                false,
            )
            .unwrap();
        let replacement = "xyλ\n".repeat(3_500_000);
        std::fs::write(&path, &replacement).unwrap();
        let loaded = FileService::read_observed_buffer_with_context(
            &path,
            true,
            buffer.observed_disk_digest.as_deref(),
            &context,
        )
        .unwrap();
        session.flush_text_chunks().unwrap();
        store.set_limits(0, 0);
        let reads = store.epoch().read_count();
        let hashed = store.hashed_bytes();
        assert!(buffer.replace_from_loaded_buffer_source(loaded, PieceSource::External));
        assert_eq!(store.epoch().read_count(), reads);
        session
            .persist(
                std::slice::from_ref(&WorkspaceTab::new(buffer.clone())),
                0,
                14.,
                false,
            )
            .unwrap();
        assert_eq!(store.hashed_bytes(), hashed);
        assert_eq!(
            buffer.document().try_content_digest().unwrap(),
            blake3::hash(replacement.as_bytes()).to_hex().as_str()
        );
        buffer.document_mut().undo_last_operation().unwrap();
        assert_eq!(
            buffer.document().try_content_digest().unwrap(),
            blake3::hash(original.as_bytes()).to_hex().as_str()
        );
    }
}
