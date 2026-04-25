use super::analysis::{IncrementalMetadataEdit, buffer_text_metadata_from_edit};
use super::{
    BufferTextMetadata, DocumentSnapshot, EncodingSource, RenderedTextWindow, TextArtifactSummary,
    TextDocument, TextDocumentOperationRecord, TextFormatMetadata, TextReplacementError,
    TextReplacements, buffer_text_metadata, buffer_text_metadata_from_piece_tree,
};
use crate::app::ui::editor_content::native_editor::CursorRange;
use eframe::egui;
use std::ops::Range;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static NEXT_BUFFER_ID: AtomicU64 = AtomicU64::new(1);
static NEXT_TEMP_BUFFER_ID: AtomicU64 = AtomicU64::new(1);

pub type BufferId = u64;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct EditorScrollOffset {
    x: f32,
    y: f32,
}

impl EditorScrollOffset {
    fn from_vec2(offset: egui::Vec2) -> Self {
        Self {
            x: sanitize_scroll_axis(offset.x),
            y: sanitize_scroll_axis(offset.y),
        }
    }

    fn to_vec2(self) -> egui::Vec2 {
        egui::vec2(self.x, self.y)
    }
}

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
    editor_scroll_offset: EditorScrollOffset,
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
        Self::build(
            next_buffer_id(),
            content,
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

    pub(crate) fn with_text_metadata(
        name: String,
        content: String,
        path: Option<PathBuf>,
        format: TextFormatMetadata,
        text_metadata: BufferTextMetadata,
    ) -> Self {
        Self::with_text_metadata_refresh_state(name, content, path, format, text_metadata, false)
    }

    pub(crate) fn with_text_metadata_refresh_state(
        name: String,
        content: String,
        path: Option<PathBuf>,
        format: TextFormatMetadata,
        text_metadata: BufferTextMetadata,
        text_metadata_refresh_stale: bool,
    ) -> Self {
        Self::build(
            next_buffer_id(),
            content,
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
        register_existing_buffer_id(restored.id);
        Self::restore_build(restored, text_metadata)
    }

    fn restore_build(restored: RestoredBufferState, text_metadata: BufferTextMetadata) -> Self {
        Self::build(
            restored.id,
            restored.content,
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

    pub fn editor_scroll_offset(&self) -> egui::Vec2 {
        self.editor_scroll_offset.to_vec2()
    }

    pub fn set_editor_scroll_offset(&mut self, offset: egui::Vec2) {
        self.editor_scroll_offset = EditorScrollOffset::from_vec2(offset);
    }

    pub fn view_status(
        &self,
        cursor_range: Option<CursorRange>,
        visible_window: Option<&RenderedTextWindow>,
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
        let (visible_line_start, visible_line_end) = visible_window
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

    pub fn visible_text_window(
        &self,
        row_range: Range<usize>,
        char_range: Range<usize>,
        total_rows: usize,
    ) -> RenderedTextWindow {
        let tree = self.document.piece_tree();
        let char_range = tree.normalize_char_range(char_range);
        let line_range = self.line_range_for_char_window(&char_range);
        let truncated_start = row_range.start > 0 || char_range.start > 0;
        let truncated_end = row_range.end < total_rows || char_range.end < tree.len_chars();

        self.build_rendered_text_window(
            row_range,
            line_range,
            char_range,
            0,
            truncated_start,
            truncated_end,
        )
    }

    pub fn visible_line_window(&self, line_range: Range<usize>) -> RenderedTextWindow {
        let max_line = self.line_count.max(1);
        let start = line_range.start.min(max_line);
        let end = line_range.end.min(max_line);
        if start >= end {
            let offset = self.document.piece_tree().len_chars();
            return self.build_rendered_text_window(
                0..0,
                start..start,
                offset..offset,
                start,
                start > 0,
                end < self.line_count,
            );
        }

        let start_char = if start < self.line_count {
            self.document.piece_tree().line_info(start).start_char
        } else {
            self.document.piece_tree().len_chars()
        };
        let end_char = if end < self.line_count {
            self.document.piece_tree().line_info(end).start_char
        } else {
            self.document.piece_tree().len_chars()
        };
        self.build_rendered_text_window(
            0..0,
            start..end,
            start_char..end_char,
            start,
            start > 0,
            end < self.line_count,
        )
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
        self.document
            .set_preferred_line_ending(self.format.preferred_line_ending_style());
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
        self.line_count = metadata.line_count;
        self.artifact_summary = metadata.artifact_summary;
        self.document
            .set_preferred_line_ending(metadata.preferred_line_ending);
        self.text_metadata_refresh_stale = false;
        self.encoding_compliance_stale = true;
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
            self.line_count = metadata.line_count;
            self.artifact_summary = metadata.artifact_summary;
            self.document
                .set_preferred_line_ending(metadata.preferred_line_ending);
            self.text_metadata_refresh_stale = false;
            self.encoding_compliance_stale = true;
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
        self.document
            .set_preferred_line_ending(self.format.preferred_line_ending_style());
        self.text_metadata_refresh_stale = false;
        self.encoding_compliance_stale = true;
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
        content: String,
        text_metadata: BufferTextMetadata,
        state: BufferBuildState,
    ) -> Self {
        Self {
            id,
            name: state.name,
            document: TextDocument::with_preferred_line_ending(
                content,
                text_metadata.preferred_line_ending,
            ),
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
            editor_scroll_offset: EditorScrollOffset::default(),
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

    fn line_range_for_char_window(&self, char_range: &Range<usize>) -> Range<usize> {
        let tree = self.document.piece_tree();
        let start = tree.char_position(char_range.start).line_index;
        let end = if char_range.is_empty() {
            start
        } else {
            tree.char_position(char_range.end.saturating_sub(1))
                .line_index
                + 1
        };
        start..end
    }

    fn build_rendered_text_window(
        &self,
        row_range: Range<usize>,
        line_range: Range<usize>,
        char_range: Range<usize>,
        layout_row_offset: usize,
        truncated_start: bool,
        truncated_end: bool,
    ) -> RenderedTextWindow {
        let text = self.document.piece_tree().extract_range(char_range.clone());
        RenderedTextWindow {
            row_range,
            line_range,
            char_range,
            layout_row_offset,
            text,
            truncated_start,
            truncated_end,
        }
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

fn sanitize_scroll_axis(axis: f32) -> f32 {
    if axis.is_finite() { axis.max(0.0) } else { 0.0 }
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
    use crate::app::domain::buffer::document::{
        TextDocumentEditOperation, TextDocumentOperationRecord,
    };
    use crate::app::domain::{LineEndingCounts, TextArtifactSummary};
    use crate::app::ui::editor_content::native_editor::CursorRange;
    use eframe::egui;

    fn selection(start: usize, end: usize) -> CursorRange {
        CursorRange::two(start, end)
    }

    #[test]
    fn visible_text_window_uses_piece_tree_coordinates() {
        let buffer = BufferState::new(
            "notes.txt".to_owned(),
            "zero\none\ntwo\nthree".to_owned(),
            None,
        );

        let window = buffer.visible_text_window(1..3, 5..13, 4);

        assert_eq!(window.row_range, 1..3);
        assert_eq!(window.line_range, 1..3);
        assert_eq!(window.char_range, 5..13);
        assert_eq!(window.layout_row_offset, 0);
        assert_eq!(window.text, "one\ntwo\n");
        assert!(window.truncated_start);
        assert!(window.truncated_end);
    }

    #[test]
    fn visible_line_window_extracts_full_lines_from_piece_tree() {
        let buffer = BufferState::new(
            "notes.txt".to_owned(),
            "zero\none\ntwo\nthree".to_owned(),
            None,
        );

        let window = buffer.visible_line_window(1..3);

        assert_eq!(window.line_range, 1..3);
        assert_eq!(window.text, "one\ntwo\n");
        assert_eq!(window.char_range, 5..13);
        assert_eq!(window.layout_row_offset, 1);
        assert!(window.truncated_start);
        assert!(window.truncated_end);
    }

    #[test]
    fn view_status_reports_piece_tree_cursor_and_viewport_coordinates() {
        let buffer = BufferState::new(
            "notes.txt".to_owned(),
            "zero\none\ntwo\nthree".to_owned(),
            None,
        );

        let visible_window = buffer.visible_line_window(1..3);
        let status = buffer.view_status(Some(selection(6, 8)), Some(&visible_window));

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

        let visible_window = buffer.visible_line_window(2..4);
        let status = buffer.view_status(None, Some(&visible_window));

        assert_eq!(status.cursor_line, None);
        assert_eq!(status.cursor_column, None);
        assert_eq!(status.selection_chars, 0);
        assert_eq!(status.visible_line_start, Some(3));
        assert_eq!(status.visible_line_end, Some(4));
    }

    #[test]
    fn editor_scroll_offset_is_buffer_owned_runtime_state() {
        let mut buffer = BufferState::new("notes.txt".to_owned(), "hello".to_owned(), None);

        assert_eq!(buffer.editor_scroll_offset(), egui::Vec2::ZERO);

        buffer.set_editor_scroll_offset(egui::vec2(18.0, 240.0));
        assert_eq!(buffer.editor_scroll_offset(), egui::vec2(18.0, 240.0));

        buffer.set_editor_scroll_offset(egui::vec2(-4.0, f32::INFINITY));
        assert_eq!(buffer.editor_scroll_offset(), egui::Vec2::ZERO);
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
