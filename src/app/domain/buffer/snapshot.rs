use super::{BufferLength, PieceTreeLite, PieceTreeSlice, display_line_count_from_piece_tree};
use std::borrow::Cow;
use std::ops::Range;
use std::sync::Arc;

#[derive(Clone)]
pub struct DocumentSnapshot {
    length: BufferLength,
    revision: u64,
    piece_tree: Arc<PieceTreeLite>,
}

impl DocumentSnapshot {
    pub(crate) fn from_shared(piece_tree: Arc<PieceTreeLite>) -> Self {
        let piece_tree = if piece_tree.has_live_anchors() {
            Arc::new(piece_tree.clone_without_anchors())
        } else {
            piece_tree
        };
        let revision = piece_tree.generation();
        let length = BufferLength::from_metrics(
            piece_tree.metrics(),
            display_line_count_from_piece_tree(piece_tree.as_ref()),
        );
        Self {
            length,
            revision,
            piece_tree,
        }
    }

    pub(crate) fn document_length(&self) -> BufferLength {
        self.length
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn piece_tree(&self) -> &PieceTreeLite {
        self.piece_tree.as_ref()
    }

    pub fn len_chars(&self) -> usize {
        self.length.chars
    }

    pub fn normalize_char_range(&self, range_chars: Range<usize>) -> Range<usize> {
        self.piece_tree.normalize_char_range(range_chars)
    }

    pub fn flatten_text(&self) -> String {
        self.piece_tree.extract_text()
    }

    pub fn flatten_range(&self, range_chars: Range<usize>) -> String {
        self.piece_tree.extract_range(range_chars)
    }

    pub fn extract_text(&self) -> String {
        self.flatten_text()
    }

    pub fn extract_range(&self, range_chars: Range<usize>) -> String {
        self.flatten_range(range_chars)
    }

    pub fn extract_range_bounded(
        &self,
        range_chars: Range<usize>,
        max_chars: usize,
    ) -> (String, bool) {
        self.piece_tree
            .extract_range_bounded(range_chars, max_chars)
    }

    pub fn spans_for_range(&self, range_chars: Range<usize>) -> PieceTreeSlice<'_> {
        self.piece_tree.spans_for_range(range_chars)
    }

    pub fn preview_for_match(&self, range_chars: &Range<usize>) -> (usize, usize, String) {
        self.piece_tree.preview_for_match(range_chars)
    }

    pub fn previews_for_matches(
        &self,
        ranges: &[Range<usize>],
        limit: usize,
    ) -> Vec<(usize, usize, String)> {
        self.piece_tree.previews_for_matches(ranges, limit)
    }

    pub fn search_text(&self, range_chars: Option<Range<usize>>) -> (String, usize) {
        let (text, start) = self.search_text_cow(range_chars);
        (text.into_owned(), start)
    }

    pub fn search_text_cow(&self, range_chars: Option<Range<usize>>) -> (Cow<'_, str>, usize) {
        let normalized = range_chars
            .map(|range_chars| self.normalize_char_range(range_chars))
            .unwrap_or_else(|| self.full_char_range());
        let start = normalized.start;
        (self.borrow_or_flatten_range(normalized), start)
    }

    fn full_char_range(&self) -> Range<usize> {
        0..self.document_length().chars
    }

    fn borrow_or_flatten_range(&self, range_chars: Range<usize>) -> Cow<'_, str> {
        self.piece_tree
            .borrow_range(range_chars.clone())
            .map(Cow::Borrowed)
            .unwrap_or_else(|| Cow::Owned(self.flatten_range(range_chars)))
    }
}

#[cfg(test)]
mod tests {
    use super::DocumentSnapshot;
    use crate::app::domain::buffer::BufferLength;
    use crate::app::domain::{AnchorBias, TextDocument};
    use crate::app::ui::editor_content::native_editor::CursorRange;
    use std::borrow::Cow;

    fn selection(start: usize, end: usize) -> CursorRange {
        CursorRange::two(start, end)
    }

    #[test]
    fn snapshot_preserves_prior_revision_after_document_mutation() {
        let mut document = TextDocument::new("alpha beta".to_owned());
        let snapshot: DocumentSnapshot = document.snapshot();

        document.insert_direct(5, "!");

        assert_eq!(snapshot.extract_text(), "alpha beta");
        assert_eq!(document.extract_text(), "alpha! beta");
        assert_ne!(snapshot.revision(), document.piece_tree().generation());
    }

    #[test]
    fn snapshot_search_text_borrows_contiguous_text() {
        let document = TextDocument::new("alpha beta".to_owned());
        let snapshot: DocumentSnapshot = document.snapshot();

        let (search_text, offset) = snapshot.search_text_cow(None);

        assert!(matches!(search_text, Cow::Borrowed("alpha beta")));
        assert_eq!(offset, 0);
    }

    #[test]
    fn snapshot_search_text_flattens_fragmented_text_when_needed() {
        let mut document = TextDocument::new("alpha beta".to_owned());
        document.insert_direct(5, "!");
        let snapshot: DocumentSnapshot = document.snapshot();

        let (search_text, offset) = snapshot.search_text_cow(None);

        assert!(matches!(search_text, Cow::Owned(_)));
        assert_eq!(search_text.as_ref(), "alpha! beta");
        assert_eq!(offset, 0);
    }

    #[test]
    fn snapshot_search_text_borrows_contiguous_subrange_after_fragmentation() {
        let mut document = TextDocument::new("abcdef".to_owned());
        document.insert_direct(3, "!");
        let snapshot: DocumentSnapshot = document.snapshot();

        let (search_text, offset) = snapshot.search_text_cow(Some(4..6));

        assert!(matches!(search_text, Cow::Borrowed("de")));
        assert_eq!(offset, 4);
    }

    #[test]
    fn snapshot_document_length_tracks_bytes_chars_and_lines() {
        let document = TextDocument::new("a\rb\r\nc".to_owned());
        let snapshot: DocumentSnapshot = document.snapshot();

        assert_eq!(
            snapshot.document_length(),
            BufferLength {
                bytes: 6,
                chars: 6,
                lines: 3,
            }
        );
    }

    #[test]
    fn snapshot_strips_live_piece_tree_anchors() {
        let mut document = TextDocument::new("alpha beta".to_owned());
        let anchor = document.piece_tree_mut().create_anchor(6, AnchorBias::Left);

        let snapshot: DocumentSnapshot = document.snapshot();

        assert_eq!(document.piece_tree().anchor_position(anchor), Some(6));
        assert_eq!(snapshot.piece_tree().anchor_position(anchor), None);
    }

    #[test]
    fn snapshot_does_not_affect_live_anchor_after_undo_redo() {
        let mut document = TextDocument::new("alpha beta gamma".to_owned());
        let anchor = document
            .piece_tree_mut()
            .create_anchor(11, AnchorBias::Left);

        document
            .replace_char_ranges_with_undo(
                &[(6..10, "BETA!".to_owned())],
                selection(6, 10),
                selection(6, 11),
            )
            .expect("replace current match");
        assert_eq!(document.extract_text(), "alpha BETA! gamma");
        assert_eq!(document.piece_tree().anchor_position(anchor), Some(12));

        let snapshot = document.snapshot();
        let snapshot_clone = snapshot.clone();
        assert_eq!(snapshot.extract_text(), "alpha BETA! gamma");
        assert_eq!(snapshot.piece_tree().anchor_position(anchor), None);
        assert_eq!(snapshot_clone.piece_tree().anchor_position(anchor), None);

        document.undo_last_operation();
        assert_eq!(document.extract_text(), "alpha beta gamma");
        assert_eq!(document.piece_tree().anchor_position(anchor), Some(11));
        assert_eq!(snapshot.extract_text(), "alpha BETA! gamma");
        assert_eq!(snapshot.piece_tree().anchor_position(anchor), None);

        document.redo_last_operation();
        assert_eq!(document.extract_text(), "alpha BETA! gamma");
        assert_eq!(document.piece_tree().anchor_position(anchor), Some(12));
        assert_eq!(snapshot_clone.extract_text(), "alpha BETA! gamma");
        assert_eq!(snapshot_clone.piece_tree().anchor_position(anchor), None);
    }
}
