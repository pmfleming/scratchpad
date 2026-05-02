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
