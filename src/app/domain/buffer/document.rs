use super::{DocumentSnapshot, LineEndingStyle, PieceTreeLite, platform_default_line_ending};
use crate::app::ui::editor_content::native_editor::{CursorRange, OperationRecord};
use std::borrow::Cow;
use std::ops::Range;
use std::sync::Arc;

pub const TEXT_DOCUMENT_MAX_UNDOS: usize = 100;
pub(crate) type TextReplacements<'a> = &'a [(Range<usize>, String)];

#[derive(Clone, Copy)]
enum OperationDirection {
    Undo,
    Redo,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextDocumentEditOperation {
    pub start_char: usize,
    pub deleted_text: String,
    pub inserted_text: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TextDocumentOperationRecord {
    pub previous_selection: CursorRange,
    pub next_selection: CursorRange,
    pub edits: Vec<TextDocumentEditOperation>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TextReplacementError {
    InvalidRange,
    OutOfBounds,
    NotDescending,
    OverlappingRanges,
}

#[derive(Clone)]
pub struct TextDocument {
    piece_tree: Arc<PieceTreeLite>,
    operation_undo: Vec<TextDocumentOperationRecord>,
    operation_redo: Vec<TextDocumentOperationRecord>,
    preferred_line_ending: LineEndingStyle,
}

impl TextDocument {
    pub fn new(text: String) -> Self {
        Self::with_preferred_line_ending(text, platform_default_line_ending())
    }

    pub fn with_preferred_line_ending(
        text: String,
        preferred_line_ending: LineEndingStyle,
    ) -> Self {
        let piece_tree = Arc::new(PieceTreeLite::from_string(text));
        Self {
            piece_tree,
            operation_undo: Vec::new(),
            operation_redo: Vec::new(),
            preferred_line_ending,
        }
    }

    /// Extract the full text content as a new String from the piece tree.
    pub fn extract_text(&self) -> String {
        self.piece_tree.extract_text()
    }

    pub fn text_cow(&self) -> Cow<'_, str> {
        self.piece_tree
            .borrow_range(0..self.piece_tree.len_chars())
            .map(Cow::Borrowed)
            .unwrap_or_else(|| Cow::Owned(self.piece_tree.extract_text()))
    }

    pub fn piece_tree(&self) -> &PieceTreeLite {
        self.piece_tree.as_ref()
    }

    /// Mutable access to the underlying piece tree. Used by view code to
    /// create/release stable anchors. Triggers `Arc::make_mut`, which clones
    /// the tree if it is currently shared (e.g. by an undo snapshot) — that
    /// is the intended copy-on-write behavior; the view's anchors must live
    /// on the new clone, not the snapshot.
    pub fn piece_tree_mut(&mut self) -> &mut PieceTreeLite {
        Arc::make_mut(&mut self.piece_tree)
    }

    pub fn snapshot(&self) -> DocumentSnapshot {
        DocumentSnapshot::from_shared(self.piece_tree.clone())
    }

    pub fn operation_undo_depth(&self) -> usize {
        self.operation_undo.len()
    }

    pub fn operation_redo_depth(&self) -> usize {
        self.operation_redo.len()
    }

    pub fn latest_operation_record(&self) -> Option<&TextDocumentOperationRecord> {
        self.operation_undo.last()
    }

    pub fn clear_operation_history(&mut self) {
        self.operation_undo.clear();
        self.operation_redo.clear();
    }

    pub fn set_preferred_line_ending(&mut self, preferred_line_ending: LineEndingStyle) {
        self.preferred_line_ending = preferred_line_ending;
    }

    pub fn replace_text(&mut self, text: String) {
        self.piece_tree = Arc::new(PieceTreeLite::from_string(text));
        self.clear_operation_history();
    }

    pub(crate) fn replace_char_ranges_with_undo(
        &mut self,
        replacements: TextReplacements<'_>,
        previous_selection: CursorRange,
        next_selection: CursorRange,
    ) -> Result<(), TextReplacementError> {
        if replacements.is_empty() {
            return Ok(());
        }

        validate_replacements(replacements, self.piece_tree.len_chars())?;

        let mut operation_record = TextDocumentOperationRecord {
            previous_selection,
            next_selection,
            edits: Vec::with_capacity(replacements.len()),
        };
        for (range, replacement) in replacements {
            let deleted_text = self.piece_tree.extract_range(range.clone());
            let normalized =
                normalize_editor_inserted_text(replacement, self.preferred_line_ending)
                    .into_owned();
            self.delete_char_range_internal(range.clone());
            self.insert_raw_text(&normalized, range.start);
            operation_record.edits.push(TextDocumentEditOperation {
                start_char: range.start,
                deleted_text,
                inserted_text: normalized,
            });
        }
        self.push_operation_record(operation_record);
        Ok(())
    }

    pub fn undo_last_operation(&mut self) -> Option<CursorRange> {
        self.replay_last_operation(OperationDirection::Undo)
    }

    pub fn redo_last_operation(&mut self) -> Option<CursorRange> {
        self.replay_last_operation(OperationDirection::Redo)
    }

    // --- Native editor direct mutation API ---

    pub fn preferred_line_ending_str(&self) -> &str {
        self.preferred_line_ending.as_str()
    }

    /// Insert text directly via piece tree.
    pub fn insert_direct(&mut self, char_index: usize, text: &str) {
        self.insert_raw_text(text, char_index);
    }

    /// Delete a char range directly via piece tree.
    pub fn delete_char_range_direct(&mut self, char_range: Range<usize>) {
        self.delete_char_range_internal(char_range);
    }

    /// Push a native operation record for undo/redo.
    pub fn push_edit_operation(&mut self, record: OperationRecord) {
        let converted = TextDocumentOperationRecord {
            previous_selection: record.previous_cursor,
            next_selection: record.next_cursor,
            edits: record
                .edits
                .into_iter()
                .map(|edit| TextDocumentEditOperation {
                    start_char: edit.start_char,
                    deleted_text: edit.deleted_text,
                    inserted_text: edit.inserted_text,
                })
                .collect(),
        };
        self.push_operation_record(converted);
    }

    fn insert_raw_text(&mut self, text: &str, char_index: usize) -> usize {
        Arc::make_mut(&mut self.piece_tree).insert(char_index, text);
        text.chars().count()
    }

    fn delete_char_range_internal(&mut self, char_range: Range<usize>) {
        assert!(
            char_range.start <= char_range.end,
            "start must be <= end, but got {char_range:?}"
        );
        Arc::make_mut(&mut self.piece_tree).remove_char_range(char_range);
    }

    fn replace_char_range_raw(&mut self, char_range: Range<usize>, replacement: &str) {
        self.delete_char_range_internal(char_range.clone());
        self.insert_raw_text(replacement, char_range.start);
    }

    fn push_operation_record(&mut self, record: TextDocumentOperationRecord) {
        if self.operation_undo.len() == TEXT_DOCUMENT_MAX_UNDOS {
            self.operation_undo.remove(0);
        }
        self.operation_undo.push(record);
        self.operation_redo.clear();
    }

    fn replay_last_operation(&mut self, direction: OperationDirection) -> Option<CursorRange> {
        let record = self.take_operation_record(direction)?;
        let selection = direction.selection(&record);
        self.apply_operation_record(&record, direction);
        self.store_replayed_operation(record, direction);
        Some(selection)
    }

    fn take_operation_record(
        &mut self,
        direction: OperationDirection,
    ) -> Option<TextDocumentOperationRecord> {
        match direction {
            OperationDirection::Undo => self.operation_undo.pop(),
            OperationDirection::Redo => self.operation_redo.pop(),
        }
    }

    fn store_replayed_operation(
        &mut self,
        record: TextDocumentOperationRecord,
        direction: OperationDirection,
    ) {
        match direction {
            OperationDirection::Undo => self.operation_redo.push(record),
            OperationDirection::Redo => self.operation_undo.push(record),
        }
    }

    fn apply_operation_record(
        &mut self,
        record: &TextDocumentOperationRecord,
        direction: OperationDirection,
    ) {
        match direction {
            OperationDirection::Undo => {
                for edit in record.edits.iter().rev() {
                    self.apply_operation_edit(
                        edit,
                        edit.inserted_text.chars().count(),
                        &edit.deleted_text,
                    );
                }
            }
            OperationDirection::Redo => {
                for edit in &record.edits {
                    self.apply_operation_edit(
                        edit,
                        edit.deleted_text.chars().count(),
                        &edit.inserted_text,
                    );
                }
            }
        }
    }

    fn apply_operation_edit(
        &mut self,
        edit: &TextDocumentEditOperation,
        replaced_len: usize,
        replacement: &str,
    ) {
        self.replace_char_range_raw(edit.start_char..edit.start_char + replaced_len, replacement);
    }
}

impl OperationDirection {
    fn selection(self, record: &TextDocumentOperationRecord) -> CursorRange {
        match self {
            OperationDirection::Undo => record.previous_selection,
            OperationDirection::Redo => record.next_selection,
        }
    }
}

fn normalize_editor_inserted_text(
    text: &str,
    preferred_line_ending: LineEndingStyle,
) -> Cow<'_, str> {
    match text {
        "\r" | "\r\n" | "\n" => Cow::Borrowed(preferred_line_ending.as_str()),
        _ if !text.contains('\n') => Cow::Borrowed(text),
        _ => {
            let replacement = preferred_line_ending.as_str();
            let mut normalized = String::with_capacity(text.len());
            let mut chars = text.chars().peekable();

            while let Some(ch) = chars.next() {
                match ch {
                    '\r' => {
                        if chars.peek() == Some(&'\n') {
                            chars.next();
                            normalized.push_str(replacement);
                        } else {
                            normalized.push(ch);
                        }
                    }
                    '\n' => normalized.push_str(replacement),
                    _ => normalized.push(ch),
                }
            }

            Cow::Owned(normalized)
        }
    }
}

fn validate_replacements(
    replacements: TextReplacements<'_>,
    text_char_len: usize,
) -> Result<(), TextReplacementError> {
    let mut previous_start = None;

    for (range, _) in replacements {
        if range.start > range.end {
            return Err(TextReplacementError::InvalidRange);
        }
        if range.end > text_char_len {
            return Err(TextReplacementError::OutOfBounds);
        }
        if let Some(last_start) = previous_start {
            if range.start > last_start {
                return Err(TextReplacementError::NotDescending);
            }
            if range.end > last_start {
                return Err(TextReplacementError::OverlappingRanges);
            }
        }
        previous_start = Some(range.start);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{TextDocument, TextReplacementError};
    use crate::app::ui::editor_content::native_editor::CursorRange;

    fn selection(start: usize, end: usize) -> CursorRange {
        CursorRange::two(start, end)
    }

    #[test]
    fn replacing_char_ranges_updates_text_and_operation_history() {
        let mut document = TextDocument::new("alpha beta".to_owned());
        let previous_selection = selection(6, 10);
        let next_selection = selection(6, 11);

        document
            .replace_char_ranges_with_undo(
                &[(6..10, "gamma".to_owned())],
                previous_selection,
                next_selection,
            )
            .expect("replace current match");

        assert_eq!(document.extract_text(), "alpha gamma");
        assert_eq!(document.operation_undo_depth(), 1);

        // Undo via operation history
        assert_eq!(document.undo_last_operation(), Some(previous_selection));
        assert_eq!(document.extract_text(), "alpha beta");

        assert_eq!(
            document
                .piece_tree()
                .extract_range(0..document.piece_tree().len_chars()),
            "alpha beta"
        );
    }

    #[test]
    fn multiple_replacements_must_be_descending() {
        let mut document = TextDocument::new("alpha beta gamma".to_owned());
        let error = document
            .replace_char_ranges_with_undo(
                &[(0..5, "omega".to_owned()), (11..16, "delta".to_owned())],
                selection(0, 5),
                selection(0, 5),
            )
            .expect_err("ascending replacements should be rejected");

        assert_eq!(error, TextReplacementError::NotDescending);
    }

    #[test]
    fn overlapping_replacements_are_rejected() {
        let mut document = TextDocument::new("alpha beta gamma".to_owned());
        let error = document
            .replace_char_ranges_with_undo(
                &[(6..10, "BETA".to_owned()), (4..8, "X".to_owned())],
                selection(6, 10),
                selection(6, 10),
            )
            .expect_err("overlapping replacements should be rejected");

        assert_eq!(error, TextReplacementError::OverlappingRanges);
    }

    #[test]
    fn piece_tree_tracks_direct_edits() {
        let mut document = TextDocument::new("héllo".to_owned());

        document.insert_direct(5, "🙂");
        document.delete_char_range_direct(1..3);

        assert_eq!(document.extract_text(), "hlo🙂");
        assert_eq!(document.piece_tree().line_lookup(0), (0, 4));
    }

    #[test]
    fn operation_history_tracks_replace_edits_without_full_text_snapshots() {
        let mut document = TextDocument::new("alpha beta gamma".to_owned());
        let previous_selection = selection(6, 10);
        let next_selection = selection(6, 11);

        document
            .replace_char_ranges_with_undo(
                &[(6..10, "BETA".to_owned())],
                previous_selection,
                next_selection,
            )
            .expect("replace current match");

        assert_eq!(document.operation_undo_depth(), 1);
        assert_eq!(document.operation_redo_depth(), 0);
    }

    #[test]
    fn operation_undo_and_redo_restore_text_and_selection() {
        let mut document = TextDocument::new("alpha beta gamma".to_owned());
        let previous_selection = selection(6, 10);
        let next_selection = selection(6, 11);

        document
            .replace_char_ranges_with_undo(
                &[(6..10, "BETA".to_owned())],
                previous_selection,
                next_selection,
            )
            .expect("replace current match");
        assert_eq!(document.extract_text(), "alpha BETA gamma");

        assert_eq!(document.undo_last_operation(), Some(previous_selection));
        assert_eq!(document.extract_text(), "alpha beta gamma");
        assert_eq!(document.operation_undo_depth(), 0);
        assert_eq!(document.operation_redo_depth(), 1);

        assert_eq!(document.redo_last_operation(), Some(next_selection));
        assert_eq!(document.extract_text(), "alpha BETA gamma");
        assert_eq!(document.operation_undo_depth(), 1);
        assert_eq!(document.operation_redo_depth(), 0);
    }

    #[test]
    fn operation_undo_handles_descending_multi_replacements() {
        let mut document = TextDocument::new("alpha beta gamma delta".to_owned());
        let previous_selection = selection(0, 5);
        let next_selection = selection(0, 5);

        document
            .replace_char_ranges_with_undo(
                &[
                    (17..22, "DELTA".to_owned()),
                    (6..10, "BETA".to_owned()),
                    (0..5, "ALPHA".to_owned()),
                ],
                previous_selection,
                next_selection,
            )
            .expect("replace multiple matches");
        assert_eq!(document.extract_text(), "ALPHA BETA gamma DELTA");

        assert_eq!(document.undo_last_operation(), Some(previous_selection));
        assert_eq!(document.extract_text(), "alpha beta gamma delta");
        assert_eq!(document.redo_last_operation(), Some(next_selection));
        assert_eq!(document.extract_text(), "ALPHA BETA gamma DELTA");
    }
}
