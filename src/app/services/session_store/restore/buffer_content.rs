use super::super::model::SessionBuffer;
use super::Restorer;
use crate::app::domain::buffer::{BufferTextMetadata, buffer_text_metadata};
use crate::app::domain::{
    BufferFreshness, BufferState, DiskFileState, RestoredBufferState, TextDocument,
    TextFormatMetadata,
};
#[cfg(test)]
use crate::app::services::file_service::FileContent;
use crate::app::services::file_service::FileService;

impl Restorer {
    #[cfg(not(test))]
    pub(super) fn restore_buffer_content(
        &mut self,
        buffer: &mut SessionBuffer,
    ) -> RestoredBufferContent {
        RestoredBufferContent::from_references(buffer)
    }
    #[cfg(test)]
    pub(super) fn restore_buffer_content(
        &mut self,
        buffer: &mut SessionBuffer,
    ) -> RestoredBufferContent {
        #[cfg(test)]
        let content = self.contents.remove(&buffer.temp_id);
        if content.is_none() && buffer.history_document.is_some() {
            return RestoredBufferContent::from_references(buffer);
        }
        let Some(path) = buffer.path.clone() else {
            return RestoredBufferContent::from_session(
                buffer,
                content.unwrap_or_default(),
                None,
                buffer.is_dirty,
                BufferFreshness::InSync,
            );
        };
        match FileService::read_observed_file(&path) {
            Ok(observed) => {
                let file = observed.content;
                let digest = observed.digest;
                let state = Some(observed.state);
                let same_text = content
                    .as_ref()
                    .is_some_and(|text| blake3::hash(text.as_bytes()).to_hex().as_str() == digest);
                let changed = buffer.disk_content_digest.as_ref().map_or_else(
                    || !same_text && state != session_disk_state(buffer),
                    |baseline| *baseline != digest,
                );
                let newly_observed = buffer.observed_disk_missing
                    || buffer
                        .observed_disk_digest
                        .as_ref()
                        .or(buffer.disk_content_digest.as_ref())
                        .is_some_and(|previous| previous != &digest)
                    || (buffer.observed_disk_digest.is_none()
                        && buffer.disk_content_digest.is_none()
                        && changed);
                let conflict = buffer.is_dirty && !same_text && (changed || newly_observed);
                buffer.pending_disk_observation = buffer.is_dirty && !same_text && newly_observed;
                buffer.observed_disk_digest = Some(digest);
                buffer.observed_disk_missing = false;
                match content {
                    Some(content) => {
                        let freshness = if conflict {
                            BufferFreshness::ConflictOnDisk
                        } else {
                            BufferFreshness::InSync
                        };
                        let mut restored = RestoredBufferContent::from_session(
                            buffer,
                            content,
                            state,
                            buffer.is_dirty && !same_text,
                            freshness,
                        );
                        if !buffer.is_dirty && !same_text {
                            restored.document.replace_exact_with_history_source(
                                &file.document.extract_text(),
                                crate::app::domain::buffer::PieceSource::External,
                            );
                            restored.format = file.format;
                            restored.text_metadata = file.text_metadata;
                        }
                        restored
                    }
                    None => RestoredBufferContent::from_file_content(
                        file,
                        state,
                        BufferFreshness::InSync,
                    ),
                }
            }
            Err(error) => {
                let missing = error.kind() == std::io::ErrorKind::NotFound;
                if missing {
                    buffer.pending_disk_observation = !buffer.observed_disk_missing;
                    buffer.observed_disk_missing = true;
                }
                RestoredBufferContent::from_session(
                    buffer,
                    content.unwrap_or_default(),
                    None,
                    buffer.is_dirty || missing,
                    if missing {
                        BufferFreshness::MissingOnDisk
                    } else {
                        BufferFreshness::StaleOnDisk
                    },
                )
            }
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
    fn from_references(buffer: &mut SessionBuffer) -> Self {
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
        Self::from_session(
            buffer,
            String::new(),
            session_disk_state(buffer),
            buffer.is_dirty || missing,
            if missing {
                BufferFreshness::MissingOnDisk
            } else if known_conflict {
                BufferFreshness::ConflictOnDisk
            } else {
                BufferFreshness::InSync
            },
        )
    }

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

    #[cfg(test)]
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
        let mut document = buffer.history_document.take().unwrap_or_else(|| {
            TextDocument::with_preferred_line_ending(content, text_metadata.preferred_line_ending)
        });
        document.set_preferred_line_ending(text_metadata.preferred_line_ending);
        if !buffer.text_history.is_empty() {
            document.restore_exported_history(std::mem::take(&mut buffer.text_history));
        }
        Self::new(
            document,
            format,
            text_metadata,
            disk_state,
            is_dirty,
            freshness,
        )
    }
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
    if let Some(metadata) = &buffer.text_metadata {
        return (buffer.format.clone(), metadata.clone());
    }
    let mut format = buffer.format.clone();
    let text_metadata = buffer_text_metadata(content, &mut format);
    (format, text_metadata)
}

pub(super) fn session_disk_state(buffer: &SessionBuffer) -> Option<DiskFileState> {
    Some(DiskFileState {
        modified_millis: buffer.disk_modified_millis,
        len: buffer.disk_len?,
    })
}
