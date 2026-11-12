use super::{
    BufferLength, PieceTreeLineInfo, PieceTreeLite, PieceTreeSlice,
    display_line_count_from_piece_tree,
};
use crate::app::capacity_metrics;
use std::borrow::Cow;
use std::ops::Range;
use std::sync::{Arc, Weak};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocumentChunk {
    pub core_range: Range<usize>,
    pub window_range: Range<usize>,
}

/// A revision number alone is not an identity: replacing a document resets it.
/// A shared logical revision survives anchor-only COW; edits and divergent clones
/// get a fresh identity. Weak ownership prevents address reuse without retaining text.
#[derive(Clone, Debug)]
pub(crate) struct SnapshotRevision {
    tree: Weak<()>,
    generation: u64,
}
impl SnapshotRevision {
    pub(super) fn new(tree: &Arc<PieceTreeLite>) -> Self {
        Self {
            tree: tree.snapshot_identity(),
            generation: tree.generation(),
        }
    }
}
impl PartialEq for SnapshotRevision {
    fn eq(&self, other: &Self) -> bool {
        self.generation == other.generation && self.tree.ptr_eq(&other.tree)
    }
}
impl Eq for SnapshotRevision {}

#[derive(Clone)]
pub struct DocumentSnapshot {
    length: BufferLength,
    revision: u64,
    source_revision: SnapshotRevision,
    piece_tree: Arc<PieceTreeLite>,
}

impl DocumentSnapshot {
    pub(crate) fn from_shared(piece_tree: Arc<PieceTreeLite>) -> Self {
        let source_revision = SnapshotRevision::new(&piece_tree);
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
            source_revision,
            piece_tree,
        }
    }

    pub(crate) fn revision_key(&self) -> SnapshotRevision {
        self.source_revision.clone()
    }

    pub(crate) fn document_length(&self) -> BufferLength {
        self.length
    }

    #[must_use]
    pub fn revision(&self) -> u64 {
        self.revision
    }

    #[must_use]
    pub fn piece_tree(&self) -> &PieceTreeLite {
        self.piece_tree.as_ref()
    }

    #[must_use]
    pub fn len_chars(&self) -> usize {
        self.length.chars
    }

    #[must_use]
    pub fn len_bytes(&self) -> usize {
        self.length.bytes
    }

    pub(crate) fn try_content_digest(&self) -> std::io::Result<String> {
        let mut hash = blake3::Hasher::new();
        for span in self.piece_tree.try_spans_for_range(0..self.len_chars()) {
            hash.update(span?.text.as_bytes());
        }
        Ok(hash.finalize().to_hex().to_string())
    }

    #[must_use]
    pub fn line_count(&self) -> usize {
        self.length.lines
    }

    #[must_use]
    pub fn line_info(&self, line_index: usize) -> PieceTreeLineInfo {
        self.piece_tree.line_info(line_index)
    }

    #[must_use]
    pub fn line_index_at_offset(&self, offset_chars: usize) -> usize {
        self.piece_tree.line_index_at_offset(offset_chars)
    }

    #[must_use]
    pub fn line_char_range(&self, line_index: usize) -> Range<usize> {
        let line = self.line_info(line_index);
        line.start_char..line.start_char + line.char_len
    }

    #[must_use]
    pub fn normalize_char_range(&self, range_chars: Range<usize>) -> Range<usize> {
        self.piece_tree.normalize_char_range(range_chars)
    }

    #[must_use]
    pub fn flatten_text(&self) -> String {
        let text = self.piece_tree.extract_text();
        capacity_metrics::record_full_text_flatten(text.len());
        text
    }

    #[must_use]
    pub fn flatten_range(&self, range_chars: Range<usize>) -> String {
        let text = self.piece_tree.extract_range(range_chars);
        capacity_metrics::record_range_flatten(text.len());
        text
    }

    #[must_use]
    pub fn extract_text(&self) -> String {
        self.flatten_text()
    }

    #[must_use]
    pub fn extract_range(&self, range_chars: Range<usize>) -> String {
        self.flatten_range(range_chars)
    }

    #[must_use]
    pub fn extract_range_bounded(
        &self,
        range_chars: Range<usize>,
        max_chars: usize,
    ) -> (String, bool) {
        self.piece_tree
            .extract_range_bounded(range_chars, max_chars)
    }

    #[must_use]
    pub fn spans_for_range(&self, range_chars: Range<usize>) -> PieceTreeSlice<'_> {
        self.piece_tree.spans_for_range(range_chars)
    }

    #[must_use]
    pub fn spans_for_line(&self, line_index: usize) -> PieceTreeSlice<'_> {
        self.piece_tree.spans_for_line(line_index)
    }

    #[must_use]
    pub fn chunks_for_range(
        &self,
        range_chars: Range<usize>,
        target_chunk_chars: usize,
        leading_context_chars: usize,
        trailing_context_chars: usize,
    ) -> Vec<DocumentChunk> {
        self.chunk_iter_for_range(
            range_chars,
            target_chunk_chars,
            leading_context_chars,
            trailing_context_chars,
        )
        .collect()
    }

    /// Plan line-aligned windows on demand so a bounded scan can reuse the
    /// bodies read while locating each boundary, instead of pre-reading a file.
    #[must_use]
    pub fn chunk_iter_for_range(
        &self,
        range_chars: Range<usize>,
        target_chunk_chars: usize,
        leading_context_chars: usize,
        trailing_context_chars: usize,
    ) -> DocumentChunks<'_> {
        let range = self.normalize_char_range(range_chars);
        DocumentChunks {
            snapshot: self,
            next: range.start,
            range,
            chunk_chars: target_chunk_chars.max(1),
            leading: leading_context_chars,
            trailing: trailing_context_chars,
        }
    }

    #[must_use]
    pub fn preview_for_match(&self, range_chars: &Range<usize>) -> (usize, usize, String) {
        super::piece_tree::preview::preview_for_match(&self.piece_tree, range_chars)
    }

    #[must_use]
    pub fn previews_for_matches(
        &self,
        ranges: &[Range<usize>],
        limit: usize,
    ) -> Vec<(usize, usize, String)> {
        super::piece_tree::preview::previews_for_matches(&self.piece_tree, ranges, limit)
    }

    #[must_use]
    pub fn search_text(&self, range_chars: Option<Range<usize>>) -> (String, usize) {
        let (text, start) = self.search_text_cow(range_chars);
        (text.into_owned(), start)
    }

    #[must_use]
    pub fn search_text_cow(&self, range_chars: Option<Range<usize>>) -> (Cow<'_, str>, usize) {
        let normalized = range_chars.map_or_else(
            || self.full_char_range(),
            |range_chars| self.normalize_char_range(range_chars),
        );
        let start = normalized.start;
        (self.borrow_or_flatten_range(normalized), start)
    }

    /// A bounded search window on the background reader, without changing the
    /// viewport cache. The returned shared/owned text is its own read pin.
    pub fn try_search_text(
        &self,
        range: Option<Range<usize>>,
    ) -> std::io::Result<(super::piece_tree::PieceTreeText<'_>, usize)> {
        let range = range.map_or(0..self.len_chars(), |range| {
            self.normalize_char_range(range)
        });
        let offset = range.start;
        let mut spans = self.piece_tree.try_spans_for_range(range.clone());
        let Some(first) = spans.next().transpose()? else {
            return Ok((
                super::piece_tree::PieceTreeText::owned(String::new()),
                offset,
            ));
        };
        if first.char_len == range.len() {
            return Ok((first.text, offset));
        }
        let mut text = first.text.to_string();
        for span in spans {
            text.push_str(&span?.text);
        }
        Ok((super::piece_tree::PieceTreeText::owned(text), offset))
    }

    fn full_char_range(&self) -> Range<usize> {
        0..self.document_length().chars
    }

    fn next_line_boundary_after(&self, offset_chars: usize, range_end: usize) -> usize {
        if offset_chars >= range_end {
            return range_end;
        }

        let line_index = self.line_index_at_offset(offset_chars);
        let next_line = line_index.saturating_add(1);
        if next_line >= self.line_count() {
            return range_end;
        }

        self.line_info(next_line).start_char.min(range_end)
    }

    fn borrow_or_flatten_range(&self, range_chars: Range<usize>) -> Cow<'_, str> {
        self.piece_tree
            .borrow_range(range_chars.clone())
            .map_or_else(
                || Cow::Owned(self.flatten_range(range_chars)),
                |text| text.into_cow(),
            )
    }
}

/// Lazy, line-aligned search windows. Constructing an iterator performs no body I/O.
pub struct DocumentChunks<'a> {
    snapshot: &'a DocumentSnapshot,
    range: Range<usize>,
    next: usize,
    chunk_chars: usize,
    leading: usize,
    trailing: usize,
}
impl Iterator for DocumentChunks<'_> {
    type Item = DocumentChunk;
    fn next(&mut self) -> Option<Self::Item> {
        let start = self.next;
        if start >= self.range.end {
            return None;
        }
        let rough_end = start.saturating_add(self.chunk_chars).min(self.range.end);
        let line_end = self
            .snapshot
            .next_line_boundary_after(rough_end, self.range.end);
        let end = if line_end > start {
            line_end
        } else {
            rough_end
        }
        .min(self.range.end);
        self.next = end;
        Some(DocumentChunk {
            core_range: start..end,
            window_range: self.range.start.max(start.saturating_sub(self.leading))
                ..self.range.end.min(end.saturating_add(self.trailing)),
        })
    }
}

#[cfg(test)]
mod chunk_iterator_tests {
    #[test]
    fn store_backed_window_planning_is_lazy_and_preserves_line_boundaries() {
        use crate::app::services::{file_service::FileService, session_store::SessionStore};
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("source");
        let text = "alpha café 東京\n".repeat(400_000);
        std::fs::write(&path, &text).unwrap();
        let session = SessionStore::new(root.path().join("session"));
        let store = session.chunk_store_handle().unwrap();
        let document = FileService::read_file_in_store(&path, store.clone())
            .unwrap()
            .document;
        session.flush_text_chunks().unwrap();
        store.set_limits(0, 0);
        store.set_limits(16 * 1024 * 1024, 4 * 1024 * 1024);
        let snapshot = document.snapshot();
        let before = store.epoch().read_count();
        let mut windows = snapshot.chunk_iter_for_range(0..snapshot.len_chars(), 256 * 1024, 5, 5);
        assert_eq!(store.epoch().read_count(), before);
        let first = windows.next().unwrap();
        assert_eq!(first.core_range.start, 0);
        assert!(
            store.epoch().read_count() - before < 8,
            "planning one window must not walk the full file"
        );
        let mut previous = first.core_range.end;
        for window in windows {
            assert_eq!(window.core_range.start, previous);
            assert!(window.window_range.start <= window.core_range.start);
            assert!(window.window_range.end >= window.core_range.end);
            previous = window.core_range.end;
            assert_eq!(
                snapshot.piece_tree().extract_range(previous - 1..previous),
                "\n"
            );
        }
        assert_eq!(previous, snapshot.len_chars());
    }
}
