use super::model::{SessionBuffer, SessionTab};
use super::{RestoreStatus, RestoreStatusLevel, SessionStore};
use crate::app::diagnostics;
use crate::app::domain::buffer::{
    BufferTextMetadata, buffer_text_metadata, detected_text_format_and_metadata,
};
use crate::app::domain::{
    BufferFreshness, BufferState, DiskFileState, EditorViewState, EncodingSource, PaneNode,
    RestoredBufferState, TextDocument, TextFormatMetadata, WorkspaceTab,
};
use crate::app::services::file_service::{FileContent, FileService};
use std::collections::HashSet;
use std::fs;
use std::path::Path;

#[derive(Default)]
pub(super) struct RestoreSummary {
    pub(super) reloaded_clean_buffers: usize,
    conflicted_buffers: usize,
    missing_buffers: usize,
}

impl RestoreSummary {
    pub(super) fn record(&mut self, freshness: BufferFreshness) {
        match freshness {
            BufferFreshness::InSync
            | BufferFreshness::AutoReloaded
            | BufferFreshness::StaleOnDisk => {}
            BufferFreshness::ConflictOnDisk => self.conflicted_buffers += 1,
            BufferFreshness::MissingOnDisk => self.missing_buffers += 1,
        }
    }

    pub(super) fn into_status(self) -> Option<RestoreStatus> {
        if self.conflicted_buffers > 0 || self.missing_buffers > 0 {
            return Some(RestoreStatus {
                level: RestoreStatusLevel::Warning,
                message: format!(
                    "Session restore found {} disk conflicts and {} missing files.",
                    self.conflicted_buffers, self.missing_buffers
                ),
            });
        }

        if self.reloaded_clean_buffers > 0 {
            return Some(RestoreStatus {
                level: RestoreStatusLevel::Info,
                message: format!(
                    "Reloaded {} clean files from disk during session restore.",
                    self.reloaded_clean_buffers
                ),
            });
        }

        None
    }
}

impl SessionStore {
    pub(super) fn restore_tab(
        &self,
        tab: SessionTab,
        summary: &mut RestoreSummary,
    ) -> WorkspaceTab {
        let mut buffers = self.restore_buffers(&tab, summary);
        let visible_control_char_buffer_ids = tab
            .views
            .iter()
            .filter(|view| view.show_control_chars)
            .map(|view| view.buffer_id)
            .collect::<HashSet<_>>();
        for buffer in &mut buffers {
            buffer.show_control_chars = buffer.artifact_summary.has_control_chars()
                && (buffer.show_control_chars
                    || visible_control_char_buffer_ids.contains(&buffer.id));
        }
        let views = tab
            .views
            .into_iter()
            .map(|view| EditorViewState::restored(view.id, view.buffer_id, view.show_line_numbers))
            .collect::<Vec<_>>();
        let root_pane = PaneNode::from(tab.root_pane);
        let active_view_id = if root_pane.contains_view(tab.active_view_id) {
            tab.active_view_id
        } else {
            root_pane.first_view_id()
        };
        let active_buffer_id = views
            .iter()
            .find(|view| view.id == active_view_id)
            .map(|view| view.buffer_id)
            .or_else(|| buffers.first().map(|buffer| buffer.id))
            .expect("restored workspace should contain at least one buffer");
        let active_buffer_index = buffers
            .iter()
            .position(|buffer| buffer.id == active_buffer_id)
            .unwrap_or(0);
        let active_buffer = buffers.remove(active_buffer_index);
        WorkspaceTab::restored_with_buffers(
            active_buffer,
            buffers,
            views,
            root_pane,
            active_view_id,
        )
    }

    fn restore_buffers(&self, tab: &SessionTab, summary: &mut RestoreSummary) -> Vec<BufferState> {
        session_buffers_for_tab(tab)
            .into_iter()
            .map(|buffer| {
                let restored = self.restore_buffer_content(&buffer);
                if !buffer.is_dirty
                    && restored.freshness == BufferFreshness::InSync
                    && restored.disk_state.is_some()
                    && restored.disk_state != session_disk_state(&buffer)
                {
                    summary.reloaded_clean_buffers += 1;
                }
                summary.record(restored.freshness);
                let mut restored_buffer = BufferState::restored_with_document_text_metadata(
                    RestoredBufferState {
                        id: buffer.id,
                        name: buffer.name,
                        content: String::new(),
                        path: buffer.path,
                        is_dirty: restored.is_dirty,
                        temp_id: buffer.temp_id,
                        format: restored.format,
                        disk_state: restored.disk_state,
                        freshness: restored.freshness,
                        show_control_chars: buffer.show_control_chars,
                        right_to_left_reading_order: buffer.right_to_left_reading_order,
                    },
                    restored.document,
                    restored.text_metadata,
                );
                restored_buffer.is_settings_file = buffer.is_settings_file;
                restored_buffer
            })
            .collect()
    }

    fn restore_buffer_content(&self, buffer: &SessionBuffer) -> RestoredBufferContent {
        let session_disk_state = session_disk_state(buffer);
        let session_path = self.buffer_path(&buffer.temp_id);
        let session_text = match fs::read_to_string(&session_path) {
            Ok(content) => Some(content),
            Err(error) => {
                diagnostics::record_io_error_with_details(
                    "session_read_buffer_snapshot",
                    Some(&session_path),
                    "session_store::restore_buffer_content",
                    &error,
                    [("temp_id", buffer.temp_id.clone())],
                );
                None
            }
        };

        match (&buffer.path, session_text) {
            (Some(path), Some(content)) => {
                self.restore_saved_buffer(buffer, path, content, session_disk_state)
            }
            (Some(path), None) => self.restore_buffer_from_disk(buffer, path),
            (None, Some(content)) => RestoredBufferContent::from_session(
                buffer,
                content,
                None,
                buffer.is_dirty,
                BufferFreshness::InSync,
            ),
            (None, None) => RestoredBufferContent::empty(buffer, BufferFreshness::InSync),
        }
    }

    fn restore_saved_buffer(
        &self,
        buffer: &SessionBuffer,
        path: &Path,
        content: String,
        session_disk_state: Option<DiskFileState>,
    ) -> RestoredBufferContent {
        match FileService::read_disk_state(path).ok() {
            Some(disk_state) if Some(disk_state.clone()) != session_disk_state => {
                self.restore_changed_disk_buffer(buffer, path, content, disk_state)
            }
            Some(disk_state) => {
                self.restore_unchanged_disk_buffer(buffer, path, content, disk_state)
            }
            None => RestoredBufferContent::from_session(
                buffer,
                content,
                None,
                buffer.is_dirty,
                BufferFreshness::MissingOnDisk,
            ),
        }
    }

    fn restore_unchanged_disk_buffer(
        &self,
        buffer: &SessionBuffer,
        path: &Path,
        content: String,
        disk_state: DiskFileState,
    ) -> RestoredBufferContent {
        if !buffer.is_dirty {
            return RestoredBufferContent::from_session(
                buffer,
                content,
                Some(disk_state),
                false,
                BufferFreshness::InSync,
            );
        }

        if disk_text_matches(path, &content) {
            return RestoredBufferContent::from_session(
                buffer,
                content,
                Some(disk_state),
                false,
                BufferFreshness::InSync,
            );
        }

        RestoredBufferContent::from_session(
            buffer,
            content,
            Some(disk_state),
            true,
            BufferFreshness::InSync,
        )
    }

    fn restore_changed_disk_buffer(
        &self,
        buffer: &SessionBuffer,
        path: &Path,
        content: String,
        disk_state: DiskFileState,
    ) -> RestoredBufferContent {
        if buffer.is_dirty {
            if disk_text_matches(path, &content) {
                return RestoredBufferContent::from_session(
                    buffer,
                    content,
                    Some(disk_state),
                    false,
                    BufferFreshness::InSync,
                );
            }

            return RestoredBufferContent::from_session(
                buffer,
                content,
                Some(disk_state),
                true,
                BufferFreshness::ConflictOnDisk,
            );
        }

        match FileService::read_file(path) {
            Ok(file_content) => RestoredBufferContent::from_file_content(
                file_content,
                Some(disk_state),
                BufferFreshness::InSync,
            ),
            Err(_) => RestoredBufferContent::from_session(
                buffer,
                content,
                Some(disk_state),
                false,
                BufferFreshness::StaleOnDisk,
            ),
        }
    }

    fn restore_buffer_from_disk(
        &self,
        buffer: &SessionBuffer,
        path: &Path,
    ) -> RestoredBufferContent {
        match FileService::read_file(path) {
            Ok(file_content) => RestoredBufferContent::from_file_content(
                file_content,
                FileService::read_disk_state(path).ok(),
                BufferFreshness::InSync,
            ),
            Err(_) => RestoredBufferContent::empty(buffer, BufferFreshness::MissingOnDisk),
        }
    }
}

struct RestoredBufferContent {
    document: TextDocument,
    format: TextFormatMetadata,
    text_metadata: BufferTextMetadata,
    disk_state: Option<DiskFileState>,
    is_dirty: bool,
    freshness: BufferFreshness,
}

impl RestoredBufferContent {
    fn new(
        document: TextDocument,
        format: TextFormatMetadata,
        text_metadata: BufferTextMetadata,
        disk_state: Option<DiskFileState>,
        is_dirty: bool,
        freshness: BufferFreshness,
    ) -> Self {
        Self {
            document,
            format,
            text_metadata,
            disk_state,
            is_dirty,
            freshness,
        }
    }

    fn from_file_content(
        file_content: FileContent,
        disk_state: Option<DiskFileState>,
        freshness: BufferFreshness,
    ) -> Self {
        Self::new(
            file_content.document,
            file_content.format,
            file_content.text_metadata,
            disk_state,
            false,
            freshness,
        )
    }

    fn from_session(
        buffer: &SessionBuffer,
        content: String,
        disk_state: Option<DiskFileState>,
        is_dirty: bool,
        freshness: BufferFreshness,
    ) -> Self {
        let (format, text_metadata) = session_buffer_format_and_metadata(buffer, &content);
        let document =
            TextDocument::with_preferred_line_ending(content, text_metadata.preferred_line_ending);
        let mut document = document;
        document.restore_exported_history(buffer.text_history.clone());
        Self::new(
            document,
            format,
            text_metadata,
            disk_state,
            is_dirty,
            freshness,
        )
    }

    fn empty(buffer: &SessionBuffer, freshness: BufferFreshness) -> Self {
        Self::from_session(buffer, String::new(), None, buffer.is_dirty, freshness)
    }
}

fn disk_text_matches(path: &Path, session_text: &str) -> bool {
    FileService::read_file(path)
        .map(|file_content| file_content.document.extract_text() == session_text)
        .unwrap_or(false)
}

fn session_buffers_for_tab(tab: &SessionTab) -> Vec<SessionBuffer> {
    if !tab.buffers.is_empty() {
        return tab.buffers.clone();
    }

    tab.buffer_id
        .zip(tab.name.clone())
        .zip(tab.is_dirty)
        .zip(tab.temp_id.clone())
        .zip(tab.encoding.clone())
        .zip(tab.has_bom)
        .map(
            |(((((buffer_id, name), is_dirty), temp_id), encoding), has_bom)| {
                vec![SessionBuffer {
                    id: buffer_id,
                    name,
                    path: tab.path.clone(),
                    is_dirty,
                    is_settings_file: false,
                    show_control_chars: false,
                    right_to_left_reading_order: false,
                    temp_id,
                    format: None,
                    encoding,
                    has_bom,
                    disk_modified_millis: None,
                    disk_len: None,
                    text_history: Vec::new(),
                }]
            },
        )
        .unwrap_or_default()
}

fn session_buffer_format_and_metadata(
    buffer: &SessionBuffer,
    content: &str,
) -> (TextFormatMetadata, BufferTextMetadata) {
    let encoding_source = if buffer.has_bom {
        EncodingSource::Bom
    } else {
        EncodingSource::Heuristic
    };

    if let Some(mut format) = buffer.format.clone() {
        let text_metadata = buffer_text_metadata(content, &mut format);
        return (format, text_metadata);
    }

    detected_text_format_and_metadata(
        content,
        buffer.encoding.clone(),
        buffer.has_bom,
        encoding_source,
        false,
    )
}

fn session_disk_state(buffer: &SessionBuffer) -> Option<DiskFileState> {
    Some(DiskFileState {
        modified_millis: buffer.disk_modified_millis,
        len: buffer.disk_len?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::services::session_store::ops::BUFFER_FILE_EXTENSION;
    use std::fs;

    #[test]
    fn stale_dirty_flag_clears_when_session_text_matches_disk() {
        let (store, buffer, _path) = restore_fixture("same", "same", true, None);

        let restored = store.restore_buffer_content(&buffer);

        assert!(!restored.is_dirty);
        assert_eq!(restored.freshness, BufferFreshness::InSync);
    }

    #[test]
    fn dirty_session_text_stays_dirty_when_disk_has_not_changed() {
        let disk_text = "disk";
        let (store, mut buffer, _path) = restore_fixture("session", disk_text, true, None);
        buffer.disk_len = Some(disk_text.len() as u64);
        buffer.disk_modified_millis = restored_disk_state(&store, &buffer).modified_millis;

        let restored = store.restore_buffer_content(&buffer);

        assert!(restored.is_dirty);
        assert_eq!(restored.freshness, BufferFreshness::InSync);
    }

    #[test]
    fn dirty_session_text_conflicts_when_disk_changed() {
        let (store, buffer, _path) = restore_fixture("session", "disk", true, None);

        let restored = store.restore_buffer_content(&buffer);

        assert!(restored.is_dirty);
        assert_eq!(restored.freshness, BufferFreshness::ConflictOnDisk);
    }

    fn restore_fixture(
        session_text: &str,
        disk_text: &str,
        is_dirty: bool,
        session_disk_len: Option<u64>,
    ) -> (SessionStore, SessionBuffer, std::path::PathBuf) {
        let temp_dir = tempfile::tempdir().expect("create temp session root");
        let root = temp_dir.keep();
        fs::create_dir_all(&root).expect("create session root");
        let path = root.join("note.txt");
        fs::write(&path, disk_text).expect("write disk file");

        let temp_id = "buffer-test".to_owned();
        fs::write(
            root.join(format!("{temp_id}.{BUFFER_FILE_EXTENSION}")),
            session_text,
        )
        .expect("write session snapshot");

        let buffer = SessionBuffer {
            id: 1,
            name: "note.txt".to_owned(),
            path: Some(path.clone()),
            is_dirty,
            is_settings_file: false,
            show_control_chars: false,
            right_to_left_reading_order: false,
            temp_id,
            format: None,
            encoding: "UTF-8".to_owned(),
            has_bom: false,
            disk_modified_millis: None,
            disk_len: session_disk_len,
            text_history: Vec::new(),
        };

        (SessionStore::new(root), buffer, path)
    }

    fn restored_disk_state(store: &SessionStore, buffer: &SessionBuffer) -> DiskFileState {
        FileService::read_disk_state(buffer.path.as_ref().expect("fixture path")).unwrap_or_else(
            |_| {
                panic!(
                    "read disk state from {}",
                    store.root().join("note.txt").display()
                )
            },
        )
    }
}
