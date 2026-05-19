use super::super::SessionStore;
use super::super::model::SessionBuffer;
use crate::app::diagnostics;
use crate::app::domain::buffer::{
    BufferTextMetadata, buffer_text_metadata, detected_text_format_and_metadata,
};
use crate::app::domain::{
    BufferFreshness, BufferState, DiskFileState, EncodingSource, RestoredBufferState, TextDocument,
    TextFormatMetadata,
};
use crate::app::services::file_service::{FileContent, FileService};
use std::fs;
use std::path::Path;

impl SessionStore {
    pub(super) fn restore_buffer_content(&self, buffer: &SessionBuffer) -> RestoredBufferContent {
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

pub(super) struct RestoredBufferContent {
    pub(super) document: TextDocument,
    pub(super) format: TextFormatMetadata,
    pub(super) text_metadata: BufferTextMetadata,
    pub(super) disk_state: Option<DiskFileState>,
    pub(super) is_dirty: bool,
    pub(super) freshness: BufferFreshness,
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

pub(super) fn cold_buffer_shell(buffer: SessionBuffer) -> BufferState {
    let (format, text_metadata) = session_buffer_format_and_metadata(&buffer, "");
    let disk_state = session_disk_state(&buffer);
    let mut restored_buffer = BufferState::restored_with_text_metadata(
        RestoredBufferState {
            id: buffer.id,
            name: buffer.name,
            content: String::new(),
            path: buffer.path,
            is_dirty: buffer.is_dirty,
            temp_id: buffer.temp_id,
            format,
            disk_state,
            freshness: BufferFreshness::InSync,
            show_control_chars: buffer.show_control_chars,
            right_to_left_reading_order: buffer.right_to_left_reading_order,
        },
        text_metadata,
    );
    restored_buffer.is_settings_file = buffer.is_settings_file;
    restored_buffer
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

pub(super) fn session_disk_state(buffer: &SessionBuffer) -> Option<DiskFileState> {
    Some(DiskFileState {
        modified_millis: buffer.disk_modified_millis,
        len: buffer.disk_len?,
    })
}
