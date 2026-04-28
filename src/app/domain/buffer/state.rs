use super::analysis::{IncrementalMetadataEdit, buffer_text_metadata_from_edit};
use super::{
    BufferLength, BufferTextMetadata, DocumentSnapshot, EncodingSource, LineEndingStyle,
    TextArtifactSummary, TextDocument, TextDocumentOperationRecord, TextFormatMetadata,
    TextReplacementError, TextReplacements, buffer_text_metadata,
    buffer_text_metadata_from_piece_tree,
};
use crate::app::domain::PublishedViewport;
use crate::app::ui::editor_content::native_editor::CursorRange;
use std::ops::Range;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static NEXT_BUFFER_ID: AtomicU64 = AtomicU64::new(1);
static NEXT_TEMP_BUFFER_ID: AtomicU64 = AtomicU64::new(1);

pub type BufferId = u64;

#[derive(Clone)]
pub struct BufferState {
    pub id: BufferId,
    pub name: String,
    document: TextDocument,
    pub path: Option<PathBuf>,
    pub is_dirty: bool,
    pub is_settings_file: bool,
    pub temp_id: String,
    pub line_count: usize,
    pub artifact_summary: TextArtifactSummary,
    pub format: TextFormatMetadata,
    pub disk_state: Option<DiskFileState>,
    pub freshness: BufferFreshness,
    pub active_selection: Option<Range<usize>>,
    pub has_non_compliant_characters: bool,
    text_metadata_refresh_stale: bool,
    encoding_compliance_stale: bool,
}

struct BufferBuildState {
    name: String,
    path: Option<PathBuf>,
    is_dirty: bool,
    temp_id: String,
    format: TextFormatMetadata,
    disk_state: Option<DiskFileState>,
    freshness: BufferFreshness,
    text_metadata_refresh_stale: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DiskFileState {
    pub modified_millis: Option<u64>,
    pub len: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BufferFreshness {
    #[default]
    InSync,
    StaleOnDisk,
    ConflictOnDisk,
    MissingOnDisk,
}

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
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BufferViewStatus {
    pub cursor_line: Option<usize>,
    pub cursor_column: Option<usize>,
    pub selection_chars: usize,
    pub visible_line_start: Option<usize>,
    pub visible_line_end: Option<usize>,
}

impl BufferState {
    pub fn new(name: String, content: String, path: Option<PathBuf>) -> Self {
        let format = TextFormatMetadata::utf8_for_new_file(&content);
        Self::with_format(name, content, path, format)
    }

    pub fn with_encoding(
        name: String,
        content: String,
        path: Option<PathBuf>,
        encoding: String,
        has_bom: bool,
    ) -> Self {
        let format = TextFormatMetadata::detected(
            &content,
            encoding,
            has_bom,
            EncodingSource::Heuristic,
            false,
        );
        Self::with_format(name, content, path, format)
    }

    pub fn with_format(
        name: String,
        content: String,
        path: Option<PathBuf>,
        mut format: TextFormatMetadata,
    ) -> Self {
        let text_metadata = buffer_text_metadata(&content, &mut format);
        let document =
            TextDocument::with_preferred_line_ending(content, text_metadata.preferred_line_ending);
        Self::build(
            next_buffer_id(),
            document,
            text_metadata,
            BufferBuildState {
                name,
                path,
                is_dirty: false,
                temp_id: next_temp_id(),
                format,
                disk_state: None,
                freshness: BufferFreshness::InSync,
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
                name,
                path,
                is_dirty: false,
                temp_id: next_temp_id(),
                format,
                disk_state: None,
                freshness: BufferFreshness::InSync,
                text_metadata_refresh_stale,
            },
        )
    }

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
                name: restored.name,
                path: restored.path,
                is_dirty: restored.is_dirty,
                temp_id: restored.temp_id,
                format: restored.format,
                disk_state: restored.disk_state,
                freshness: restored.freshness,
                text_metadata_refresh_stale: false,
            },
        )
    }

    pub fn document(&self) -> &TextDocument {
        &self.document
    }

    pub fn document_mut(&mut self) -> &mut TextDocument {
        &mut self.document
    }

    pub fn text(&self) -> String {
        self.document.extract_text()
    }

    pub fn preview_for_match(&self, range: &Range<usize>) -> (usize, usize, String) {
        self.document.piece_tree().preview_for_match(range)
    }

    pub fn document_snapshot(&self) -> DocumentSnapshot {
        self.document.snapshot()
    }

    pub fn document_revision(&self) -> u64 {
        self.document.piece_tree().generation()
    }

    pub(crate) fn current_file_length(&self) -> BufferLength {
        BufferLength::from_metrics(self.document.piece_tree().metrics(), self.line_count)
    }

    pub fn view_status(
        &self,
        cursor_range: Option<CursorRange>,
        published_viewport: Option<&PublishedViewport>,
    ) -> BufferViewStatus {
        let (cursor_line, cursor_column, selection_chars) = cursor_range
            .map(|range| {
                let position = self
                    .document
                    .piece_tree()
                    .char_position(range.primary.index);
                (
                    Some(position.line_index + 1),
                    Some(position.column_index + 1),
                    range.primary.index.abs_diff(range.secondary.index),
                )
            })
            .unwrap_or((None, None, 0));
        let (visible_line_start, visible_line_end) = published_viewport
            .and_then(|window| {
                (!window.line_range.is_empty()).then_some((
                    Some(window.line_range.start + 1),
                    Some(window.line_range.end),
                ))
            })
            .unwrap_or((None, None));

        BufferViewStatus {
            cursor_line,
            cursor_column,
            selection_chars,
            visible_line_start,
            visible_line_end,
        }
    }

    /// Extract the text covered by the given 0-indexed logical line range
    /// directly from the piece tree. Used by consumers that need the raw text
    /// of a viewport slice (artifact mode, split previews).
    pub fn extract_text_for_lines(&self, line_range: Range<usize>) -> String {
        let max_line = self.line_count.max(1);
        let start = line_range.start.min(max_line);
        let end = line_range.end.min(max_line);
        if start >= end {
            return String::new();
        }
        let tree = self.document.piece_tree();
        let start_char = if start < self.line_count {
            tree.line_info(start).start_char
        } else {
            tree.len_chars()
        };
        let end_char = if end < self.line_count {
            tree.line_info(end).start_char
        } else {
            tree.len_chars()
        };
        tree.extract_range(start_char..end_char)
    }

    pub fn replace_text(&mut self, text: String) {
        self.replace_document_text(text, None);
    }

    pub fn replace_text_with_format(&mut self, text: String, format: TextFormatMetadata) {
        self.replace_document_text(text, Some(format));
    }

    pub fn replace_from_loaded_buffer(&mut self, loaded: BufferState) {
        self.name = loaded.name;
        self.document = loaded.document;
        self.path = loaded.path;
        self.line_count = loaded.line_count;
        self.artifact_summary = loaded.artifact_summary;
        self.format = loaded.format;
        self.disk_state = loaded.disk_state;
        self.freshness = loaded.freshness;
        self.active_selection = None;
        self.has_non_compliant_characters = loaded.has_non_compliant_characters;
        self.text_metadata_refresh_stale = loaded.text_metadata_refresh_stale;
        self.encoding_compliance_stale = loaded.encoding_compliance_stale;
    }

    pub fn replace_format_without_text_change(&mut self, format: TextFormatMetadata) {
        self.format = format;
        self.sync_document_preferred_line_ending();
        self.encoding_compliance_stale = true;
    }

    pub(crate) fn replace_char_ranges_with_undo(
        &mut self,
        replacements: TextReplacements<'_>,
        previous_selection: CursorRange,
        next_selection: CursorRange,
    ) -> Result<(), TextReplacementError> {
        self.document.replace_char_ranges_with_undo(
            replacements,
            previous_selection,
            next_selection,
        )?;
        self.refresh_text_metadata();
        Ok(())
    }

    pub fn undo_last_text_operation(&mut self) -> Option<CursorRange> {
        let selection = self.document.undo_last_operation()?;
        self.refresh_text_metadata();
        Some(selection)
    }

    pub fn redo_last_text_operation(&mut self) -> Option<CursorRange> {
        let selection = self.document.redo_last_operation()?;
        self.refresh_text_metadata();
        Some(selection)
    }

    pub fn refresh_text_metadata(&mut self) {
        let metadata =
            buffer_text_metadata_from_piece_tree(self.document.piece_tree(), &mut self.format);
        self.apply_text_metadata(metadata);
    }

    pub fn refresh_text_metadata_after_operation(
        &mut self,
        operation: Option<&TextDocumentOperationRecord>,
    ) {
        if self.text_metadata_refresh_stale {
            self.refresh_text_metadata();
            return;
        }

        if operation.is_some_and(|operation| self.can_skip_metadata_rescan(operation)) {
            return;
        }

        if let Some(metadata) = operation
            .and_then(|operation| self.incremental_text_metadata_after_operation(operation))
        {
            self.apply_text_metadata(metadata);
            return;
        }

        self.refresh_text_metadata();
    }

    pub fn recheck_encoding_compliance(&mut self) {
        if !self.encoding_compliance_stale {
            return;
        }
        let tree = self.document.piece_tree();
        self.has_non_compliant_characters = self.format.has_non_compliant_characters_spans(
            tree.spans_for_range(0..tree.len_chars()).map(|s| s.text),
        );
        self.encoding_compliance_stale = false;
    }

    pub fn encoding_compliance_refresh_needed(&self) -> bool {
        self.encoding_compliance_stale
    }

    pub fn text_metadata_refresh_needed(&self) -> bool {
        self.text_metadata_refresh_stale
    }

    pub fn apply_encoding_compliance_refresh(&mut self, revision: u64, has_non_compliant: bool) {
        if self.document_revision() != revision {
            return;
        }
        self.has_non_compliant_characters = has_non_compliant;
        self.encoding_compliance_stale = false;
    }

    pub fn apply_text_metadata_refresh(
        &mut self,
        revision: u64,
        line_count: usize,
        artifact_summary: TextArtifactSummary,
        format: TextFormatMetadata,
    ) {
        if self.document_revision() != revision {
            return;
        }

        self.line_count = line_count;
        self.artifact_summary = artifact_summary;
        self.format = format;
        self.apply_text_metadata_fields(
            line_count,
            self.artifact_summary.clone(),
            self.format.preferred_line_ending_style(),
        );
    }

    pub fn sync_to_disk_state(&mut self, disk_state: Option<DiskFileState>) {
        self.set_disk_state(disk_state, BufferFreshness::InSync);
    }

    pub fn mark_stale_on_disk(&mut self, disk_state: Option<DiskFileState>) {
        self.set_disk_state(disk_state, BufferFreshness::StaleOnDisk);
    }

    pub fn mark_conflict_on_disk(&mut self, disk_state: Option<DiskFileState>) {
        self.set_disk_state(disk_state, BufferFreshness::ConflictOnDisk);
    }

    pub fn mark_missing_on_disk(&mut self) {
        self.freshness = BufferFreshness::MissingOnDisk;
    }

    pub fn disk_status_label(&self) -> Option<&'static str> {
        match self.freshness {
            BufferFreshness::InSync => None,
            BufferFreshness::StaleOnDisk => Some("On disk changed"),
            BufferFreshness::ConflictOnDisk => Some("Disk conflict"),
            BufferFreshness::MissingOnDisk => Some("File missing"),
        }
    }

    pub fn disk_status_message(&self) -> Option<String> {
        let path_label = self
            .path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| self.name.clone());

        match self.freshness {
            BufferFreshness::InSync => None,
            BufferFreshness::StaleOnDisk => Some(format!("{path_label} changed on disk.")),
            BufferFreshness::ConflictOnDisk => Some(format!(
                "{path_label} changed on disk. Your tab has unsaved edits."
            )),
            BufferFreshness::MissingOnDisk => Some(format!("{path_label} is missing on disk.")),
        }
    }

    pub fn display_name(&self) -> String {
        if self.is_dirty {
            format!("*{}", self.name)
        } else {
            self.name.clone()
        }
    }

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
            name: state.name,
            document,
            path: state.path,
            is_dirty: state.is_dirty,
            is_settings_file: false,
            temp_id: state.temp_id,
            line_count: text_metadata.line_count,
            artifact_summary: text_metadata.artifact_summary,
            format: state.format,
            disk_state: state.disk_state,
            freshness: state.freshness,
            active_selection: None,
            has_non_compliant_characters: text_metadata.has_non_compliant_characters,
            text_metadata_refresh_stale: state.text_metadata_refresh_stale,
            encoding_compliance_stale: false,
        }
    }

    fn replace_document_text(&mut self, text: String, format: Option<TextFormatMetadata>) {
        self.document.replace_text(text);
        if let Some(format) = format {
            self.format = format;
        }
        self.refresh_text_metadata();
    }

    fn apply_text_metadata(&mut self, metadata: BufferTextMetadata) {
        self.apply_text_metadata_fields(
            metadata.line_count,
            metadata.artifact_summary,
            metadata.preferred_line_ending,
        );
    }

    fn apply_text_metadata_fields(
        &mut self,
        line_count: usize,
        artifact_summary: TextArtifactSummary,
        preferred_line_ending: LineEndingStyle,
    ) {
        self.line_count = line_count;
        self.artifact_summary = artifact_summary;
        self.document
            .set_preferred_line_ending(preferred_line_ending);
        self.text_metadata_refresh_stale = false;
        self.encoding_compliance_stale = true;
    }

    fn sync_document_preferred_line_ending(&mut self) {
        self.document
            .set_preferred_line_ending(self.format.preferred_line_ending_style());
    }

    fn can_skip_metadata_rescan(&self, operation: &TextDocumentOperationRecord) -> bool {
        self.format.is_ascii_subset
            && !self.artifact_summary.has_control_chars()
            && operation.edits.iter().all(|edit| {
                metadata_neutral_ascii_text(&edit.deleted_text)
                    && metadata_neutral_ascii_text(&edit.inserted_text)
            })
    }

    fn incremental_text_metadata_after_operation(
        &mut self,
        operation: &TextDocumentOperationRecord,
    ) -> Option<BufferTextMetadata> {
        if operation.edits.len() != 1 {
            return None;
        }

        let edit = operation.edits.first()?;
        let tree = self.document.piece_tree();
        let start_char = edit.start_char.min(tree.len_chars());
        let inserted_char_len = edit.inserted_text.chars().count();
        let previous_char = start_char
            .checked_sub(1)
            .and_then(|index| tree.char_at(index));
        let next_char = tree.char_at(start_char.saturating_add(inserted_char_len));

        buffer_text_metadata_from_edit(
            self.line_count,
            &self.artifact_summary,
            &mut self.format,
            IncrementalMetadataEdit {
                previous_char,
                deleted_text: &edit.deleted_text,
                inserted_text: &edit.inserted_text,
                next_char,
            },
        )
    }

    fn set_disk_state(&mut self, disk_state: Option<DiskFileState>, freshness: BufferFreshness) {
        self.disk_state = disk_state;
        self.freshness = freshness;
    }
}

fn metadata_neutral_ascii_text(text: &str) -> bool {
    text.bytes()
        .all(|byte| byte.is_ascii() && !matches!(byte, b'\n' | b'\r' | 0x00..=0x1F))
}

fn next_buffer_id() -> BufferId {
    NEXT_BUFFER_ID.fetch_add(1, Ordering::Relaxed)
}

fn register_existing_buffer_id(id: BufferId) {
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

#[cfg(test)]
mod tests {
    use super::{BufferState, metadata_neutral_ascii_text};
    use crate::app::domain::PublishedViewport;
    use crate::app::domain::buffer::BufferLength;
    use crate::app::domain::buffer::document::{
        TextDocumentEditOperation, TextDocumentOperationRecord,
    };
    use crate::app::domain::{LineEndingCounts, TextArtifactSummary};
    use crate::app::ui::editor_content::native_editor::CursorRange;

    fn selection(start: usize, end: usize) -> CursorRange {
        CursorRange::two(start, end)
    }

    fn published_viewport(line_range: std::ops::Range<usize>) -> PublishedViewport {
        PublishedViewport {
            row_range: 0..line_range.len(),
            line_range: line_range.clone(),
            layout_row_offset: line_range.start,
        }
    }

    #[test]
    fn extract_text_for_lines_returns_lines_from_piece_tree() {
        let buffer = BufferState::new(
            "notes.txt".to_owned(),
            "zero\none\ntwo\nthree".to_owned(),
            None,
        );

        assert_eq!(buffer.extract_text_for_lines(1..3), "one\ntwo\n");
        assert_eq!(buffer.extract_text_for_lines(0..1), "zero\n");
        assert_eq!(buffer.extract_text_for_lines(5..7), "");
    }

    #[test]
    fn extract_text_for_lines_handles_final_partial_line() {
        // Document does not end with a newline — last logical line has no trailing \n.
        let buffer = BufferState::new(
            "notes.txt".to_owned(),
            "alpha\nbeta\ngamma".to_owned(),
            None,
        );
        // Asking for the last line yields the trailing fragment without a newline.
        assert_eq!(buffer.extract_text_for_lines(2..3), "gamma");
        // Asking for the whole document returns the full text.
        assert_eq!(buffer.extract_text_for_lines(0..3), "alpha\nbeta\ngamma");
    }

    #[test]
    fn extract_text_for_lines_clamps_to_document_bounds() {
        let buffer = BufferState::new("notes.txt".to_owned(), "one\ntwo".to_owned(), None);
        // Range that overruns end is clamped.
        assert_eq!(buffer.extract_text_for_lines(0..100), "one\ntwo");
        // Empty range yields empty string.
        assert_eq!(buffer.extract_text_for_lines(1..1), "");
        // Inverted range yields empty string.
        #[allow(clippy::reversed_empty_ranges)]
        let inverted = 2..1;
        assert_eq!(buffer.extract_text_for_lines(inverted), "");
    }

    #[test]
    fn view_status_reports_piece_tree_cursor_and_viewport_coordinates() {
        let buffer = BufferState::new(
            "notes.txt".to_owned(),
            "zero\none\ntwo\nthree".to_owned(),
            None,
        );

        let window = published_viewport(1..3);
        let status = buffer.view_status(Some(selection(6, 8)), Some(&window));

        assert_eq!(status.cursor_line, Some(2));
        assert_eq!(status.cursor_column, Some(4));
        assert_eq!(status.selection_chars, 2);
        assert_eq!(status.visible_line_start, Some(2));
        assert_eq!(status.visible_line_end, Some(3));
    }

    #[test]
    fn view_status_still_reports_visible_lines_without_a_cursor() {
        let buffer = BufferState::new(
            "notes.txt".to_owned(),
            "zero\none\ntwo\nthree".to_owned(),
            None,
        );

        let window = published_viewport(2..4);
        let status = buffer.view_status(None, Some(&window));

        assert_eq!(status.cursor_line, None);
        assert_eq!(status.cursor_column, None);
        assert_eq!(status.selection_chars, 0);
        assert_eq!(status.visible_line_start, Some(3));
        assert_eq!(status.visible_line_end, Some(4));
    }

    #[test]
    fn current_file_length_tracks_bytes_chars_and_lines() {
        let buffer = BufferState::new("notes.txt".to_owned(), "hi\nworld".to_owned(), None);

        assert_eq!(
            buffer.current_file_length(),
            BufferLength {
                bytes: 8,
                chars: 8,
                lines: 2,
            }
        );
    }

    #[test]
    fn ascii_metadata_neutral_operation_skips_full_rescan() {
        let mut buffer = BufferState::new("notes.txt".to_owned(), "hello".to_owned(), None);
        buffer.document_mut().insert_direct(5, "!");
        let operation = operation_record(5, "", "!");
        buffer.line_count = 99;

        buffer.refresh_text_metadata_after_operation(Some(&operation));

        assert_eq!(buffer.line_count, 99);
        assert_eq!(buffer.artifact_summary, TextArtifactSummary::default());
        assert!(buffer.format.is_ascii_subset);
    }

    #[test]
    fn control_character_operation_falls_back_to_full_metadata_rescan() {
        let mut buffer = BufferState::new("notes.txt".to_owned(), "hello".to_owned(), None);
        buffer.document_mut().insert_direct(5, "\u{1b}");
        let operation = operation_record(5, "", "\u{1b}");
        buffer.line_count = 99;

        buffer.refresh_text_metadata_after_operation(Some(&operation));

        assert_eq!(buffer.line_count, 1);
        assert!(buffer.artifact_summary.has_ansi_sequences);
    }

    #[test]
    fn newline_operation_updates_line_metadata_incrementally() {
        let mut buffer = BufferState::new("notes.txt".to_owned(), "hello".to_owned(), None);
        buffer.document_mut().insert_direct(5, "\nworld");
        let operation = operation_record(5, "", "\nworld");
        buffer.line_count = 99;

        buffer.refresh_text_metadata_after_operation(Some(&operation));

        assert_eq!(buffer.line_count, 100);
        assert_eq!(
            buffer.format.line_ending_counts,
            LineEndingCounts {
                lf: 1,
                crlf: 0,
                cr: 0
            }
        );
    }

    #[test]
    fn newline_operation_updates_crlf_boundaries_incrementally() {
        let mut buffer = BufferState::new("notes.txt".to_owned(), "hello\rworld".to_owned(), None);
        buffer.document_mut().insert_direct(6, "\n");
        let operation = operation_record(6, "", "\n");
        buffer.line_count = 41;

        buffer.refresh_text_metadata_after_operation(Some(&operation));

        assert_eq!(buffer.line_count, 41);
        assert_eq!(
            buffer.format.line_ending_counts,
            LineEndingCounts {
                lf: 0,
                crlf: 1,
                cr: 0
            }
        );
    }

    #[test]
    fn metadata_neutral_ascii_rejects_control_and_line_endings() {
        assert!(metadata_neutral_ascii_text("abcXYZ123 "));
        assert!(!metadata_neutral_ascii_text("abc\n"));
        assert!(!metadata_neutral_ascii_text("abc\r"));
        assert!(!metadata_neutral_ascii_text("abc\t"));
        assert!(!metadata_neutral_ascii_text("abcé"));
    }

    fn operation_record(
        start_char: usize,
        deleted_text: &str,
        inserted_text: &str,
    ) -> TextDocumentOperationRecord {
        let selection = selection(start_char, start_char);
        TextDocumentOperationRecord {
            previous_selection: selection,
            next_selection: selection,
            edits: vec![TextDocumentEditOperation {
                start_char,
                deleted_text: deleted_text.to_owned(),
                inserted_text: inserted_text.to_owned(),
            }],
        }
    }
}
