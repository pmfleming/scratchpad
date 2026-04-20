use super::{
    BufferTextMetadata, EncodingSource, RenderedTextWindow, TextArtifactSummary, TextDocument,
    TextFormatMetadata, TextReplacementError, TextReplacements, buffer_text_metadata,
    buffer_text_metadata_from_piece_tree,
};
use crate::app::ui::editor_content::native_editor::CursorRange;
use eframe::egui;
use std::cell::RefCell;
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
    encoding_compliance_stale: bool,
    cached_full_text: RefCell<Option<(u64, String)>>,
}

struct BufferBuildState {
    name: String,
    path: Option<PathBuf>,
    is_dirty: bool,
    temp_id: String,
    format: TextFormatMetadata,
    disk_state: Option<DiskFileState>,
    freshness: BufferFreshness,
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
            },
        )
    }

    pub fn restored(restored: RestoredBufferState) -> Self {
        register_existing_buffer_id(restored.id);
        let mut format = restored.format;
        let text_metadata = buffer_text_metadata(&restored.content, &mut format);
        Self::build(
            restored.id,
            restored.content,
            text_metadata,
            BufferBuildState {
                name: restored.name,
                path: restored.path,
                is_dirty: restored.is_dirty,
                temp_id: restored.temp_id,
                format,
                disk_state: restored.disk_state,
                freshness: restored.freshness,
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

    pub fn search_text_snapshot(&self, range: Option<Range<usize>>) -> (String, usize) {
        let Some(range) = range else {
            let current_gen = self.document.piece_tree().generation();
            {
                let cache = self.cached_full_text.borrow();
                if let Some((cached_gen, ref text)) = *cache
                    && cached_gen == current_gen
                {
                    return (text.clone(), 0);
                }
            }
            let text = self.document.extract_text();
            *self.cached_full_text.borrow_mut() = Some((current_gen, text.clone()));
            return (text, 0);
        };

        let normalized = self.document.piece_tree().normalize_char_range(range);
        (
            self.document.piece_tree().extract_range(normalized.clone()),
            normalized.start,
        )
    }

    pub fn visible_text_window(
        &self,
        row_range: Range<usize>,
        char_range: Range<usize>,
        total_rows: usize,
    ) -> RenderedTextWindow {
        let normalized = self.document.piece_tree().normalize_char_range(char_range);
        let line_start = self
            .document
            .piece_tree()
            .char_position(normalized.start)
            .line_index;
        let line_end = if normalized.is_empty() {
            line_start
        } else {
            self.document
                .piece_tree()
                .char_position(normalized.end.saturating_sub(1))
                .line_index
                + 1
        };

        RenderedTextWindow {
            row_range: row_range.clone(),
            line_range: line_start..line_end,
            char_range: normalized.clone(),
            layout_row_offset: 0,
            text: self.document.piece_tree().extract_range(normalized.clone()),
            truncated_start: row_range.start > 0 || normalized.start > 0,
            truncated_end: row_range.end < total_rows
                || normalized.end < self.document.piece_tree().len_chars(),
        }
    }

    pub fn visible_line_window(&self, line_range: Range<usize>) -> RenderedTextWindow {
        let max_line = self.line_count.max(1);
        let start = line_range.start.min(max_line);
        let end = line_range.end.min(max_line);
        if start >= end {
            let offset = self.document.piece_tree().len_chars();
            return RenderedTextWindow {
                row_range: 0..0,
                line_range: start..start,
                char_range: offset..offset,
                layout_row_offset: start,
                text: String::new(),
                truncated_start: start > 0,
                truncated_end: end < self.line_count,
            };
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
        let char_range = start_char..end_char;

        RenderedTextWindow {
            row_range: 0..0,
            line_range: start..end,
            char_range: char_range.clone(),
            layout_row_offset: start,
            text: self.document.piece_tree().extract_range(char_range),
            truncated_start: start > 0,
            truncated_end: end < self.line_count,
        }
    }

    pub fn replace_text(&mut self, text: String) {
        self.replace_document_text(text, None);
    }

    pub fn replace_text_with_format(&mut self, text: String, format: TextFormatMetadata) {
        self.replace_document_text(text, Some(format));
    }

    pub(crate) fn replace_char_ranges_with_undo(
        &mut self,
        replacements: TextReplacements<'_>,
        previous_selection: egui::text::CCursorRange,
        next_selection: egui::text::CCursorRange,
    ) -> Result<(), TextReplacementError> {
        self.document.replace_char_ranges_with_undo(
            replacements,
            previous_selection,
            next_selection,
        )?;
        self.refresh_text_metadata();
        Ok(())
    }

    pub fn undo_last_text_operation(&mut self) -> Option<egui::text::CCursorRange> {
        let selection = self.document.undo_last_operation()?;
        self.refresh_text_metadata();
        Some(selection)
    }

    pub fn redo_last_text_operation(&mut self) -> Option<egui::text::CCursorRange> {
        let selection = self.document.redo_last_operation()?;
        self.refresh_text_metadata();
        Some(selection)
    }

    pub fn undo_last_text_operation_native(&mut self) -> Option<CursorRange> {
        let selection = self.document.undo_operation_native()?;
        self.refresh_text_metadata();
        Some(selection)
    }

    pub fn redo_last_text_operation_native(&mut self) -> Option<CursorRange> {
        let selection = self.document.redo_operation_native()?;
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
        self.encoding_compliance_stale = true;
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
            has_non_compliant_characters: text_metadata.has_non_compliant_characters,
            encoding_compliance_stale: false,
            cached_full_text: RefCell::new(None),
        }
    }

    fn replace_document_text(&mut self, text: String, format: Option<TextFormatMetadata>) {
        self.document.replace_text(text);
        if let Some(format) = format {
            self.format = format;
        }
        self.refresh_text_metadata();
    }

    fn set_disk_state(&mut self, disk_state: Option<DiskFileState>, freshness: BufferFreshness) {
        self.disk_state = disk_state;
        self.freshness = freshness;
    }
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
    use super::BufferState;
    use crate::app::ui::editor_content::native_editor::CursorRange;

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
}
