use super::{PieceTreeLite, PieceTreeSlice};
use std::borrow::Cow;
use std::ops::Range;
use std::sync::Arc;

#[derive(Clone)]
pub struct DocumentSnapshot {
    revision: u64,
    piece_tree: Arc<PieceTreeLite>,
}

impl DocumentSnapshot {
    pub(crate) fn from_shared(piece_tree: Arc<PieceTreeLite>) -> Self {
        let revision = piece_tree.generation();
        Self {
            revision,
            piece_tree,
        }
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn piece_tree(&self) -> &PieceTreeLite {
        self.piece_tree.as_ref()
    }

    pub fn len_chars(&self) -> usize {
        self.piece_tree.len_chars()
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
        let Some(range_chars) = range_chars else {
            return (
                self.piece_tree
                    .borrow_range(0..self.len_chars())
                    .map(Cow::Borrowed)
                    .unwrap_or_else(|| Cow::Owned(self.flatten_text())),
                0,
            );
        };

        let normalized = self.normalize_char_range(range_chars);
        let start = normalized.start;
        (
            self.piece_tree
                .borrow_range(normalized.clone())
                .map(Cow::Borrowed)
                .unwrap_or_else(|| Cow::Owned(self.flatten_range(normalized))),
            start,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::DocumentSnapshot;
    use crate::app::domain::TextDocument;
    use std::borrow::Cow;

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
}
