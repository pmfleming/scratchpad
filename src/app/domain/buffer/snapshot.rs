use super::{
    BufferLength, PieceTreeLineInfo, PieceTreeLite, PieceTreeSlice,
    display_line_count_from_piece_tree,
};
use crate::app::capacity_metrics;
use std::borrow::Cow;
use std::ops::Range;
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocumentChunk {
    pub core_range: Range<usize>,
    pub window_range: Range<usize>,
}

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

    pub fn len_bytes(&self) -> usize {
        self.length.bytes
    }

    pub fn line_count(&self) -> usize {
        self.length.lines
    }

    pub fn line_info(&self, line_index: usize) -> PieceTreeLineInfo {
        self.piece_tree.line_info(line_index)
    }

    pub fn line_index_at_offset(&self, offset_chars: usize) -> usize {
        self.piece_tree.line_index_at_offset(offset_chars)
    }

    pub fn line_char_range(&self, line_index: usize) -> Range<usize> {
        let line = self.line_info(line_index);
        line.start_char..line.start_char + line.char_len
    }

    pub fn normalize_char_range(&self, range_chars: Range<usize>) -> Range<usize> {
        self.piece_tree.normalize_char_range(range_chars)
    }

    pub fn flatten_text(&self) -> String {
        let text = self.piece_tree.extract_text();
        capacity_metrics::record_full_text_flatten(text.len());
        text
    }

    pub fn flatten_range(&self, range_chars: Range<usize>) -> String {
        let text = self.piece_tree.extract_range(range_chars);
        capacity_metrics::record_range_flatten(text.len());
        text
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

    pub fn spans_for_line(&self, line_index: usize) -> PieceTreeSlice<'_> {
        self.piece_tree.spans_for_line(line_index)
    }

    pub fn chunks_for_range(
        &self,
        range_chars: Range<usize>,
        target_chunk_chars: usize,
        leading_context_chars: usize,
        trailing_context_chars: usize,
    ) -> Vec<DocumentChunk> {
        let normalized = self.normalize_char_range(range_chars);
        if normalized.is_empty() {
            return Vec::new();
        }

        let chunk_chars = target_chunk_chars.max(1);
        let mut chunks = Vec::new();
        let mut chunk_start = normalized.start;

        while chunk_start < normalized.end {
            let rough_end = chunk_start.saturating_add(chunk_chars).min(normalized.end);
            let core_end = self
                .next_line_boundary_after(rough_end, normalized.end)
                .filter(|line_start| *line_start > chunk_start)
                .unwrap_or(rough_end)
                .min(normalized.end);
            let window_start = normalized
                .start
                .max(chunk_start.saturating_sub(leading_context_chars));
            let window_end = normalized
                .end
                .min(core_end.saturating_add(trailing_context_chars));
            chunks.push(DocumentChunk {
                core_range: chunk_start..core_end,
                window_range: window_start..window_end,
            });
            chunk_start = core_end;
        }

        chunks
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

    fn next_line_boundary_after(&self, offset_chars: usize, range_end: usize) -> Option<usize> {
        if offset_chars >= range_end {
            return Some(range_end);
        }

        let line_index = self.line_index_at_offset(offset_chars);
        let next_line = line_index.saturating_add(1);
        if next_line >= self.line_count() {
            return Some(range_end);
        }

        Some(self.line_info(next_line).start_char.min(range_end))
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
    fn snapshot_exposes_line_metadata_and_line_spans() {
        let mut document = TextDocument::new("alpha\nbravo\ncharlie".to_owned());
        document.insert_direct(6, "wide ");
        let snapshot = document.snapshot();

        assert_eq!(snapshot.line_count(), 3);
        assert_eq!(snapshot.line_index_at_offset(8), 1);
        assert_eq!(snapshot.line_char_range(1), 6..16);

        let line_text = snapshot
            .spans_for_line(1)
            .map(|span| span.text)
            .collect::<String>();
        assert_eq!(line_text, "wide bravo");
    }

    #[test]
    fn snapshot_chunks_align_core_ranges_to_line_boundaries() {
        let document = TextDocument::new("aaaa\nbbbb\ncccc\ndddd\neeee".to_owned());
        let snapshot = document.snapshot();

        let chunks = snapshot.chunks_for_range(0..snapshot.len_chars(), 7, 2, 3);

        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].core_range, 0..10);
        assert_eq!(chunks[0].window_range, 0..13);
        assert_eq!(chunks[1].core_range, 10..20);
        assert_eq!(chunks[1].window_range, 8..23);
        assert_eq!(chunks[2].core_range, 20..24);
        assert_eq!(chunks[2].window_range, 18..24);
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
