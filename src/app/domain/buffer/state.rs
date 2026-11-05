use super::analysis::{
    IncrementalMetadataEdit, IncrementalMetadataUpdate, buffer_text_metadata_from_edit,
};
use super::{
    BufferLength, BufferTextMetadata, DocumentSnapshot, EncodingSource, LineEndingStyle,
    PieceSource, TextArtifactSummary, TextDocument, TextDocumentOperationRecord,
    TextFormatMetadata, TextHistoryApplyError, TextReplacementError, TextReplacements,
    buffer_text_metadata, buffer_text_metadata_from_piece_tree,
};
use crate::app::CanonicalPathKey;
use crate::app::domain::{CursorRange, buffer::OperationRecord};
use std::ops::Range;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

mod disk;
mod metadata;
#[cfg(test)]
mod tests;

static NEXT_BUFFER_ID: AtomicU64 = AtomicU64::new(1);
static NEXT_TEMP_BUFFER_ID: AtomicU64 = AtomicU64::new(1);

pub type BufferId = u64;

#[derive(Clone)]
pub struct BufferState {
    pub id: BufferId,
    document: TextDocument,
    pub name: String,
    pub path: Option<PathBuf>,
    pub path_key: Option<CanonicalPathKey>,
    pub is_dirty: bool,
    pub is_settings_file: bool,
    /// True while this buffer contains only the bounded first-visible prefix of
    /// a large file and full hydration is still running.
    pub is_loading_preview: bool,
    pub show_control_chars: bool,
    pub right_to_left_reading_order: bool,
    pub temp_id: String,
    pub line_count: usize,
    pub artifact_summary: TextArtifactSummary,
    pub format: TextFormatMetadata,
    pub disk_state: Option<DiskFileState>,
    /// Digest of decoded text at the last successful disk synchronization.
    pub disk_content_digest: Option<String>,
    /// Last disk version actually read; differs from the saved baseline during a conflict.
    pub(crate) observed_disk_digest: Option<String>,
    pub(crate) observed_disk_missing: bool,
    pub(crate) pending_disk_observation: bool,
    pub(crate) pending_reopen_external: Option<u64>,
    pub(crate) pending_reopen_original: Option<Box<TextDocument>>,
    pub(crate) loaded_replacement_text: Option<std::sync::Arc<String>>,
    pub freshness: BufferFreshness,
    pub active_selection: Option<Range<usize>>,
    pub has_non_compliant_characters: bool,
    refresh: BufferRefreshState,
}

#[derive(Clone)]
struct BufferRefreshState {
    text_metadata_refresh_stale: bool,
    line_ending_metadata_exact: bool,
    encoding_compliance_stale: bool,
    pending_text_history_event: Option<(super::HistoryToken, TextHistoryEvent)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TextHistoryEvent {
    Edit,
    Replay,
    Navigate,
}

struct BufferBuildState {
    name: String,
    path: Option<PathBuf>,
    path_key: Option<CanonicalPathKey>,
    is_dirty: bool,
    temp_id: String,
    format: TextFormatMetadata,
    disk_state: Option<DiskFileState>,
    freshness: BufferFreshness,
    show_control_chars: bool,
    right_to_left_reading_order: bool,
    text_metadata_refresh_stale: bool,
}

pub use disk::{BufferFreshness, DiskFileState};

pub struct RestoredBufferState {
    pub id: BufferId,
    pub name: String,
    pub content: String,
    pub path: Option<PathBuf>,
    pub is_dirty: bool,
    pub temp_id: String,
    pub format: TextFormatMetadata,
    pub disk_state: Option<DiskFileState>,
    pub freshness: BufferFreshness,
    pub show_control_chars: bool,
    pub right_to_left_reading_order: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BufferViewStatus {
    pub cursor_line: Option<usize>,
    pub cursor_column: Option<usize>,
    pub selection_chars: usize,
}

impl BufferState {
    #[must_use]
    pub fn new(name: String, content: String, path: Option<PathBuf>) -> Self {
        let (format, text_metadata) = super::detected_text_format_and_metadata(
            &content,
            "UTF-8".to_owned(),
            false,
            EncodingSource::DefaultForNewFile,
            false,
        );
        Self::with_format_and_metadata(name, content, path, format, text_metadata)
    }

    #[must_use]
    pub fn with_encoding(
        name: String,
        content: String,
        path: Option<PathBuf>,
        encoding: String,
        has_bom: bool,
    ) -> Self {
        let (format, text_metadata) = super::detected_text_format_and_metadata(
            &content,
            encoding,
            has_bom,
            EncodingSource::Heuristic,
            false,
        );
        Self::with_format_and_metadata(name, content, path, format, text_metadata)
    }

    fn with_format_and_metadata(
        name: String,
        content: String,
        path: Option<PathBuf>,
        format: TextFormatMetadata,
        text_metadata: BufferTextMetadata,
    ) -> Self {
        let document =
            TextDocument::with_preferred_line_ending(content, text_metadata.preferred_line_ending);
        Self::build(
            next_buffer_id(),
            document,
            text_metadata,
            BufferBuildState {
                path_key: path.as_deref().map(CanonicalPathKey::from_path),
                name,
                path,
                is_dirty: false,
                temp_id: next_temp_id(),
                format,
                disk_state: None,
                freshness: BufferFreshness::InSync,
                show_control_chars: false,
                right_to_left_reading_order: false,
                text_metadata_refresh_stale: false,
            },
        )
    }

    pub(crate) fn with_document_text_metadata_refresh_state(
        name: String,
        document: TextDocument,
        path: Option<PathBuf>,
        format: TextFormatMetadata,
        text_metadata: BufferTextMetadata,
        text_metadata_refresh_stale: bool,
    ) -> Self {
        Self::build(
            next_buffer_id(),
            document,
            text_metadata,
            BufferBuildState {
                path_key: path.as_deref().map(CanonicalPathKey::from_path),
                name,
                path,
                is_dirty: false,
                temp_id: next_temp_id(),
                format,
                disk_state: None,
                freshness: BufferFreshness::InSync,
                show_control_chars: false,
                right_to_left_reading_order: false,
                text_metadata_refresh_stale,
            },
        )
    }

    #[must_use]
    pub fn restored(restored: RestoredBufferState) -> Self {
        let mut format = restored.format;
        let text_metadata = buffer_text_metadata(&restored.content, &mut format);
        let restored = RestoredBufferState { format, ..restored };
        Self::restored_with_text_metadata(restored, text_metadata)
    }

    pub(crate) fn restored_with_text_metadata(
        restored: RestoredBufferState,
        text_metadata: BufferTextMetadata,
    ) -> Self {
        let document = TextDocument::with_preferred_line_ending(
            restored.content.clone(),
            text_metadata.preferred_line_ending,
        );
        Self::restored_with_document_text_metadata(restored, document, text_metadata)
    }

    pub(crate) fn restored_with_document_text_metadata(
        restored: RestoredBufferState,
        document: TextDocument,
        text_metadata: BufferTextMetadata,
    ) -> Self {
        register_existing_buffer_id(restored.id);
        Self::restore_build(restored, document, text_metadata)
    }

    fn restore_build(
        restored: RestoredBufferState,
        document: TextDocument,
        text_metadata: BufferTextMetadata,
    ) -> Self {
        Self::build(
            restored.id,
            document,
            text_metadata,
            BufferBuildState {
                path_key: restored.path.as_deref().map(CanonicalPathKey::from_path),
                name: restored.name,
                path: restored.path,
                is_dirty: restored.is_dirty,
                temp_id: restored.temp_id,
                format: restored.format,
                disk_state: restored.disk_state,
                freshness: restored.freshness,
                show_control_chars: restored.show_control_chars,
                right_to_left_reading_order: restored.right_to_left_reading_order,
                text_metadata_refresh_stale: false,
            },
        )
    }

    #[must_use]
    pub fn document(&self) -> &TextDocument {
        &self.document
    }

    pub fn document_mut(&mut self) -> &mut TextDocument {
        &mut self.document
    }

    #[must_use]
    pub fn text(&self) -> String {
        self.document.extract_text()
    }

    #[must_use]
    pub fn preview_for_match(&self, range: &Range<usize>) -> (usize, usize, String) {
        super::piece_tree::preview::preview_for_match(self.document.piece_tree(), range)
    }

    #[must_use]
    pub fn document_snapshot(&self) -> DocumentSnapshot {
        self.document.snapshot()
    }

    #[must_use]
    pub fn document_revision(&self) -> u64 {
        self.document.piece_tree().generation()
    }

    pub(crate) fn current_file_length(&self) -> BufferLength {
        BufferLength::from_metrics(self.document.piece_tree().metrics(), self.line_count)
    }

    pub(crate) fn has_visible_control_substitutions(&self) -> bool {
        self.line_count > 1
            || self.artifact_summary.has_control_chars()
            || self
                .document
                .piece_tree()
                .spans_for_range(0..self.document.piece_tree().len_chars())
                .any(|span| span.text.contains('\t'))
    }

    #[must_use]
    pub fn view_status(&self, cursor_range: Option<CursorRange>) -> BufferViewStatus {
        let (cursor_line, cursor_column, selection_chars) =
            cursor_range.map_or((None, None, 0), |range| {
                let position = super::piece_tree::query::char_position(
                    self.document.piece_tree(),
                    range.primary.index,
                );
                (
                    Some(position.line_index + 1),
                    Some(position.column_index + 1),
                    range.primary.index.abs_diff(range.secondary.index),
                )
            });

        BufferViewStatus {
            cursor_line,
            cursor_column,
            selection_chars,
        }
    }

    pub fn replace_text(&mut self, text: String) {
        self.document.replace_text(text);
        self.refresh_text_metadata();
    }

    pub fn mark_as_loading_preview(&mut self) {
        self.is_loading_preview = true;
    }

    pub fn replace_from_loaded_buffer(&mut self, loaded: BufferState) -> bool {
        let source = if self
            .disk_content_digest
            .as_ref()
            .zip(loaded.disk_content_digest.as_ref())
            .is_some_and(|(saved, digest)| saved != digest)
        {
            super::PieceSource::External
        } else {
            super::PieceSource::Load
        };
        self.replace_from_loaded_buffer_source(loaded, source)
    }

    pub(crate) fn replace_from_loaded_buffer_source(
        &mut self,
        mut loaded: BufferState,
        source: super::PieceSource,
    ) -> bool {
        let id = self.id;
        let temp_id = self.temp_id.clone();
        let history = if self.is_loading_preview {
            None
        } else {
            let text = if let Some(text) = loaded.loaded_replacement_text.take() {
                text
            } else {
                let mut text = String::new();
                for span in loaded
                    .document
                    .piece_tree()
                    .try_spans_for_range(0..loaded.document.piece_tree().len_chars())
                {
                    let Ok(span) = span else { return false };
                    text.push_str(&span.text);
                }
                std::sync::Arc::new(text)
            };
            if !self
                .document
                .replace_exact_with_history_source(&text, source)
            {
                return false;
            }
            Some(self.document.clone())
        };
        *self = loaded;
        self.id = id;
        self.temp_id = temp_id;
        if let Some(document) = history {
            self.document = document;
        }
        self.active_selection = None;
        self.refresh.pending_text_history_event = None;
        true
    }

    /// Preserve recovered text/undo without retaining an obsolete disk baseline.
    pub(crate) fn recover_as_untitled(&mut self, label: &str) {
        self.set_path(None);
        self.disk_state = None;
        self.disk_content_digest = None;
        self.freshness = BufferFreshness::InSync;
        self.is_settings_file = false;
        self.is_dirty = true;
        self.name = format!("{} ({label})", self.name);
    }

    pub fn set_path(&mut self, path: Option<PathBuf>) {
        self.path_key = path.as_deref().map(CanonicalPathKey::from_path);
        self.path = path;
    }

    pub fn replace_format_without_text_change(&mut self, format: TextFormatMetadata) {
        self.format = format;
        self.sync_document_preferred_line_ending();
        self.refresh.encoding_compliance_stale = true;
    }

    pub(crate) fn replace_char_ranges_with_undo(
        &mut self,
        replacements: TextReplacements<'_>,
        previous_selection: CursorRange,
        next_selection: CursorRange,
    ) -> Result<(), TextReplacementError> {
        if self.is_loading_preview {
            return Err(TextReplacementError::InvalidRange);
        }
        self.document.replace_char_ranges_with_undo(
            replacements,
            previous_selection,
            next_selection,
        )?;
        self.note_text_history_event(TextHistoryEvent::Edit);
        let latest_edit = self.document.latest_operation_record().cloned();
        self.refresh_text_metadata_after_operation(latest_edit.as_ref());
        Ok(())
    }

    pub(crate) fn push_text_edit_operation_with_source(
        &mut self,
        record: OperationRecord,
        source: PieceSource,
    ) {
        self.document
            .push_edit_operation_with_source(record, source);
        self.note_text_history_event(TextHistoryEvent::Edit);
    }

    /// Latest input wins, including undo boundaries and IME before text commits.
    pub(crate) fn note_text_history_event(&mut self, event: TextHistoryEvent) {
        self.refresh.pending_text_history_event = Some((self.document.history_token(), event));
    }

    pub(crate) fn take_text_history_event(&mut self) -> Option<TextHistoryEvent> {
        self.refresh
            .pending_text_history_event
            .take()
            .filter(|(token, _)| *token == self.document.history_token())
            .map(|(_, event)| event)
    }

    pub(crate) fn validate_char_replacements(
        &self,
        replacements: TextReplacements<'_>,
    ) -> Result<(), TextReplacementError> {
        self.document.validate_char_replacements(replacements)
    }

    pub fn undo_last_text_operation(&mut self) -> Option<CursorRange> {
        let selection = self.document.undo_last_operation()?;
        self.note_text_history_event(TextHistoryEvent::Replay);
        let latest_edit = self.document.latest_operation_record().cloned();
        self.refresh_text_metadata_after_operation(latest_edit.as_ref());
        Some(selection)
    }

    pub(crate) fn navigate_history(
        &mut self,
        target: u64,
    ) -> Result<CursorRange, TextHistoryApplyError> {
        let selection = self.document.navigate_history(target)?;
        self.note_text_history_event(TextHistoryEvent::Navigate);
        self.refresh_text_metadata();
        self.mark_dirty_after_local_edit();
        Ok(selection)
    }

    pub fn redo_last_text_operation(&mut self) -> Option<CursorRange> {
        let selection = self.document.redo_last_operation()?;
        self.note_text_history_event(TextHistoryEvent::Replay);
        let latest_edit = self.document.latest_operation_record().cloned();
        self.refresh_text_metadata_after_operation(latest_edit.as_ref());
        Some(selection)
    }

    #[must_use]
    pub fn display_name(&self) -> String {
        self.name.clone()
    }

    #[must_use]
    pub fn overflow_context_label(&self) -> Option<String> {
        self.path.as_ref().map(|path| path.display().to_string())
    }

    fn build(
        id: BufferId,
        document: TextDocument,
        text_metadata: BufferTextMetadata,
        state: BufferBuildState,
    ) -> Self {
        Self {
            id,
            document,
            name: state.name,
            path: state.path,
            path_key: state.path_key,
            is_dirty: state.is_dirty,
            is_settings_file: false,
            is_loading_preview: false,
            show_control_chars: state.show_control_chars,
            right_to_left_reading_order: state.right_to_left_reading_order,
            temp_id: state.temp_id,
            line_count: text_metadata.line_count,
            artifact_summary: text_metadata.artifact_summary,
            format: state.format,
            disk_state: state.disk_state,
            disk_content_digest: None,
            observed_disk_digest: None,
            observed_disk_missing: false,
            pending_disk_observation: false,
            pending_reopen_external: None,
            pending_reopen_original: None,
            loaded_replacement_text: None,
            freshness: state.freshness,
            active_selection: None,
            has_non_compliant_characters: text_metadata.has_non_compliant_characters,
            refresh: BufferRefreshState {
                text_metadata_refresh_stale: state.text_metadata_refresh_stale,
                line_ending_metadata_exact: !state.text_metadata_refresh_stale,
                encoding_compliance_stale: false,
                pending_text_history_event: None,
            },
        }
    }
}

fn next_buffer_id() -> BufferId {
    NEXT_BUFFER_ID.fetch_add(1, Ordering::Relaxed)
}

pub(crate) fn register_existing_buffer_id(id: BufferId) {
    let next_id = id.saturating_add(1);
    let mut current = NEXT_BUFFER_ID.load(Ordering::Relaxed);

    while current < next_id {
        match NEXT_BUFFER_ID.compare_exchange(
            current,
            next_id,
            Ordering::Relaxed,
            Ordering::Relaxed,
        ) {
            Ok(_) => break,
            Err(observed) => current = observed,
        }
    }
}

fn next_temp_id() -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let sequence = NEXT_TEMP_BUFFER_ID.fetch_add(1, Ordering::Relaxed);
    format!("buffer-{timestamp}-{sequence}")
}
