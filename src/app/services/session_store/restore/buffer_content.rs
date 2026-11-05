use super::super::model::SessionBuffer;
use crate::app::domain::buffer::{BufferTextMetadata, buffer_text_metadata};
use crate::app::domain::{
    BufferFreshness, BufferState, DiskFileState, RestoredBufferState, TextDocument,
    TextFormatMetadata,
};
use crate::app::services::file_service::FileService;
use std::io;

pub(super) struct RestoredBufferContent {
    pub(super) document: TextDocument,
    pub(super) format: TextFormatMetadata,
    pub(super) text_metadata: BufferTextMetadata,
    pub(super) disk_state: Option<DiskFileState>,
    pub(super) is_dirty: bool,
    pub(super) freshness: BufferFreshness,
}

impl RestoredBufferContent {
    pub(super) fn from_references(buffer: &mut SessionBuffer) -> io::Result<Self> {
        let mut document = buffer.history_document.take().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("missing restored document {}", buffer.temp_id),
            )
        })?;
        let (format, text_metadata) = session_buffer_format_and_metadata(buffer);
        document.set_preferred_line_ending(text_metadata.preferred_line_ending);
        let missing = buffer.path.as_ref().is_some_and(|path| {
            FileService::read_disk_state(path)
                .is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound)
        });
        let known_conflict = buffer.is_dirty
            && buffer
                .observed_disk_digest
                .as_ref()
                .zip(buffer.disk_content_digest.as_ref())
                .is_some_and(|(observed, baseline)| observed != baseline);
        if missing {
            buffer.pending_disk_observation = !buffer.observed_disk_missing;
            buffer.observed_disk_missing = true;
        }
        Ok(Self {
            document,
            format,
            text_metadata,
            disk_state: session_disk_state(buffer),
            is_dirty: buffer.is_dirty || missing,
            freshness: if missing {
                BufferFreshness::MissingOnDisk
            } else if known_conflict {
                BufferFreshness::ConflictOnDisk
            } else {
                BufferFreshness::InSync
            },
        })
    }
}

pub(super) fn cold_buffer_shell(buffer: &SessionBuffer) -> BufferState {
    let (format, text_metadata) = session_buffer_format_and_metadata(buffer);
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
) -> (TextFormatMetadata, BufferTextMetadata) {
    if let Some(metadata) = &buffer.text_metadata {
        return (buffer.format.clone(), metadata.clone());
    }
    let mut format = buffer.format.clone();
    let text_metadata = buffer_text_metadata("", &mut format);
    (format, text_metadata)
}

pub(super) fn session_disk_state(buffer: &SessionBuffer) -> Option<DiskFileState> {
    Some(DiskFileState {
        modified_millis: buffer.disk_modified_millis,
        len: buffer.disk_len?,
    })
}
