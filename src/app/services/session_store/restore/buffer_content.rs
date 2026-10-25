use super::super::model::SessionBuffer;
use super::Restorer;
use crate::app::domain::buffer::{
    BufferTextMetadata, buffer_text_metadata, detected_text_format_and_metadata,
};
use crate::app::domain::{
    BufferFreshness, BufferState, DiskFileState, EncodingSource, RestoredBufferState, TextDocument,
    TextFormatMetadata,
};
use crate::app::services::file_service::{FileContent, FileService};
use std::path::Path;

impl Restorer {
    pub(super) fn restore_buffer_content(
        &mut self,
        buffer: &mut SessionBuffer,
    ) -> RestoredBufferContent {
        let is_dirty = buffer.is_dirty;
        let session_disk_state = session_disk_state(buffer);
        let session_text = self.contents.remove(&buffer.temp_id);
        let path = buffer.path.take();
        let restored = match (path.as_deref(), session_text) {
            (Some(path), Some(content)) => {
                self.restore_saved_buffer(buffer, path, content, session_disk_state)
            }
            (Some(path), None) => self.restore_buffer_from_disk(buffer, path),
            (None, Some(content)) => RestoredBufferContent::from_session(
                buffer,
                content,
                None,
                is_dirty,
                BufferFreshness::InSync,
            ),
            (None, None) => RestoredBufferContent::empty(buffer, BufferFreshness::InSync),
        };
        buffer.path = path;
        restored
    }

    fn restore_saved_buffer(
        &self,
        buffer: &mut SessionBuffer,
        path: &Path,
        content: String,
        session_disk_state: Option<DiskFileState>,
    ) -> RestoredBufferContent {
        let is_dirty = buffer.is_dirty;
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
                is_dirty,
                BufferFreshness::MissingOnDisk,
            ),
        }
    }

    fn restore_unchanged_disk_buffer(
        &self,
        buffer: &mut SessionBuffer,
        path: &Path,
        content: String,
        disk_state: DiskFileState,
    ) -> RestoredBufferContent {
        let is_dirty = buffer.is_dirty && !disk_text_matches(path, &content);
        RestoredBufferContent::from_session(
            buffer,
            content,
            Some(disk_state),
            is_dirty,
            BufferFreshness::InSync,
        )
    }

    fn restore_changed_disk_buffer(
        &self,
        buffer: &mut SessionBuffer,
        path: &Path,
        content: String,
        disk_state: DiskFileState,
    ) -> RestoredBufferContent {
        if buffer.is_dirty {
            let is_dirty = !disk_text_matches(path, &content);
            let freshness = if is_dirty {
                BufferFreshness::ConflictOnDisk
            } else {
                BufferFreshness::InSync
            };
            return RestoredBufferContent::from_session(
                buffer,
                content,
                Some(disk_state),
                is_dirty,
                freshness,
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
        buffer: &mut SessionBuffer,
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
        buffer: &mut SessionBuffer,
        content: String,
        disk_state: Option<DiskFileState>,
        is_dirty: bool,
        freshness: BufferFreshness,
    ) -> Self {
        let (format, text_metadata) = session_buffer_format_and_metadata(buffer, &content);
        let mut document =
            TextDocument::with_preferred_line_ending(content, text_metadata.preferred_line_ending);
        document.restore_exported_history(std::mem::take(&mut buffer.text_history));
        Self::new(
            document,
            format,
            text_metadata,
            disk_state,
            is_dirty,
            freshness,
        )
    }

    fn empty(buffer: &mut SessionBuffer, freshness: BufferFreshness) -> Self {
        let is_dirty = buffer.is_dirty;
        Self::from_session(buffer, String::new(), None, is_dirty, freshness)
    }
}

fn disk_text_matches(path: &Path, session_text: &str) -> bool {
    FileService::read_file(path)
        .is_ok_and(|file_content| file_content.document.extract_text() == session_text)
}

pub(super) fn cold_buffer_shell(buffer: &SessionBuffer) -> BufferState {
    let (format, text_metadata) = session_buffer_format_and_metadata(buffer, "");
    let disk_state = session_disk_state(buffer);
    let mut restored_buffer = BufferState::restored_with_text_metadata(
        RestoredBufferState {
            id: buffer.id,
            name: buffer.name.clone(),
            content: String::new(),
            path: buffer.path.clone(),
            is_dirty: buffer.is_dirty,
            temp_id: buffer.temp_id.clone(),
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
