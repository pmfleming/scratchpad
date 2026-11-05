//! Logical edit-tree export, not a raw copy of a potentially live database/WAL.
use super::{
    SessionStore,
    model::{SessionBuffer, SessionTab},
};
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufWriter, Write};
use std::path::Path;

impl SessionStore {
    /// Export one committed session to a new private directory. Existing paths
    /// are never overwritten. The manifest is written last; an error can leave
    /// an incomplete directory, which must not be treated as a valid backup.
    pub fn export_recovery(&self, destination: &Path) -> io::Result<()> {
        // Export is a recovery operation, not an interactive fresh start. Never
        // discard an incompatible source database while trying to back it up.
        self.worker.get_or_init(|| {
            super::DatabaseWorker::open(self.root.clone()).map_err(|error| error.to_string())
        });
        let (stored, retained, closed) = self
            .worker()?
            .call(|db| futures_lite::future::block_on(db.recovery_metadata()))?;
        let mut directory = fs::DirBuilder::new();
        directory.recursive(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            directory.mode(0o700);
        }
        directory.create(destination)?;
        // All metadata came from one actor job. Immutable handles retain roots
        // while payloads stream, even if a later save changes the live workspace.
        for buffer in stored
            .manifest
            .tabs
            .iter()
            .flat_map(|tab| &tab.buffers)
            .chain(&retained)
        {
            write_new(
                &destination.join(format!("{}.tmp", buffer.temp_id)),
                |file| {
                    if let Some(document) = &buffer.history_document {
                        let snapshot = document.snapshot();
                        for span in snapshot
                            .piece_tree()
                            .try_spans_for_range(0..snapshot.len_chars())
                        {
                            file.write_all(span?.text.as_bytes())?;
                        }
                    } else if !buffer.text_is_empty {
                        return Err(io::Error::other(format!(
                            "Missing text for {}",
                            buffer.temp_id
                        )));
                    }
                    Ok(())
                },
            )?;
        }
        write_new(&destination.join("session.json"), |file| {
            let metadata = serde_json::json!({"version":stored.manifest.version,"tab_count":stored.manifest.tab_count,"active_tab_index":stored.manifest.active_tab_index,"active_surface":stored.manifest.active_surface,"history_fingerprint":"blake3-u64-le-v1"});
            write_prefix(file, &metadata)?;
            file.write_all(b",\"tabs\":[")?;
            for (index, tab) in stored.manifest.tabs.iter().enumerate() {
                if index > 0 {
                    file.write_all(b",")?;
                }
                write_tab(file, tab)?;
            }
            file.write_all(b"],\"retained_documents\":[")?;
            for (index, buffer) in retained.iter().enumerate() {
                if index > 0 {
                    file.write_all(b",")?;
                }
                write_buffer(file, buffer)?;
            }
            file.write_all(b"],\"closed_tabs\":[")?;
            for (index, (token, closed_at, tab)) in closed.iter().enumerate() {
                if index > 0 {
                    file.write_all(b",")?;
                }
                serde_json::to_writer(
                    &mut *file,
                    &serde_json::json!({"token":token,"closed_at":closed_at,"layout":tab}),
                )
                .map_err(io::Error::other)?;
            }
            file.write_all(b"]}")
        })?;
        #[cfg(unix)]
        {
            File::open(destination)?.sync_all()?;
            let parent = destination
                .parent()
                .filter(|path| !path.as_os_str().is_empty())
                .unwrap_or(Path::new("."));
            File::open(parent)?.sync_all()?;
        }
        Ok(())
    }
}

fn write_new(
    path: &Path,
    write: impl FnOnce(&mut BufWriter<File>) -> io::Result<()>,
) -> io::Result<()> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = BufWriter::new(options.open(path)?);
    write(&mut file)?;
    file.flush()?;
    file.get_ref().sync_all()
}

fn write_prefix(writer: &mut impl Write, value: &serde_json::Value) -> io::Result<()> {
    let bytes = serde_json::to_vec(value).map_err(io::Error::other)?;
    writer.write_all(&bytes[..bytes.len() - 1])
}
fn write_tab(writer: &mut impl Write, tab: &SessionTab) -> io::Result<()> {
    let mut metadata = serde_json::to_value(tab).map_err(io::Error::other)?;
    metadata
        .as_object_mut()
        .expect("tab object")
        .remove("buffers");
    write_prefix(writer, &metadata)?;
    writer.write_all(b",\"buffers\":[")?;
    for (index, buffer) in tab.buffers.iter().enumerate() {
        if index > 0 {
            writer.write_all(b",")?;
        }
        write_buffer(writer, buffer)?;
    }
    writer.write_all(b"]}")
}
fn write_buffer(writer: &mut impl Write, buffer: &SessionBuffer) -> io::Result<()> {
    let mut metadata = serde_json::to_value(buffer).map_err(io::Error::other)?;
    metadata
        .as_object_mut()
        .expect("buffer object")
        .remove("text_history");
    write_prefix(writer, &metadata)?;
    writer.write_all(b",\"text_history\":")?;
    if let Some(document) = &buffer.history_document {
        document.write_recovery_history(writer)?;
    } else {
        serde_json::to_writer(&mut *writer, &buffer.text_history).map_err(io::Error::other)?;
    }
    writer.write_all(b"}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::domain::{BufferState, PersistedHistoryEntry, WorkspaceTab};
    use crate::app::services::session_store::{CapturedClosedTab, SessionPersistRequest};
    #[test]
    fn export_includes_closed_retained_text_and_exact_history_without_consuming_the_archive() {
        let root = tempfile::tempdir().unwrap();
        let session = SessionStore::new(root.path().join("store"));
        let tabs = vec![
            WorkspaceTab::new(BufferState::new("open".into(), "visible".into(), None)),
            WorkspaceTab::new(BufferState::new("closed".into(), "λ\0🦀\r\n".into(), None)),
        ];
        session.persist(&tabs, 0, 14., false).unwrap();
        let mut request = SessionPersistRequest::capture(&tabs, 0, 14., false);
        request.close_tab(1, CapturedClosedTab::capture(&tabs[1], None));
        session.persist_request(request).unwrap();
        let destination = root.path().join("export");
        session.export_recovery(&destination).unwrap();
        let exported: serde_json::Value =
            serde_json::from_slice(&fs::read(destination.join("session.json")).unwrap()).unwrap();
        assert_eq!(exported["tabs"].as_array().unwrap().len(), 1);
        assert_eq!(exported["closed_tabs"].as_array().unwrap().len(), 1);
        let retained = &exported["retained_documents"][0];
        assert_eq!(
            fs::read_to_string(
                destination.join(format!("{}.tmp", retained["temp_id"].as_str().unwrap()))
            )
            .unwrap(),
            "λ\0🦀\r\n"
        );
        let rows: Vec<PersistedHistoryEntry> =
            serde_json::from_value(retained["text_history"].clone()).unwrap();
        assert!(rows.iter().all(PersistedHistoryEntry::payloads_valid));
        assert_eq!(session.closed_tabs().unwrap().len(), 1);
    }
}
