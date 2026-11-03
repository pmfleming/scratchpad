mod history_ops;
mod recovery;
mod tree_ops;

use super::history::{OperationDirection, TextDocumentEditOperation, TextHistoryApplyError};
use super::{
    ByteSpan, DocumentSnapshot, LineEndingStyle, PersistedHistoryEntry, PieceHistoryEdit,
    PieceHistoryEntry, PieceSource, PieceTreeLite, TextDocumentOperationRecord, TextHistoryBudget,
    fingerprint_parts, normalize_inserted_text_line_endings, platform_default_line_ending,
};
use crate::app::capacity_metrics;
use crate::app::domain::{CursorRange, buffer::OperationRecord};
use std::io;
use std::ops::Range;
use std::path::Path;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use std::time::Instant;

pub(crate) type TextReplacements<'a> = &'a [(Range<usize>, String)];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TextReplacementError {
    InvalidRange,
    OutOfBounds,
    NotDescending,
    OverlappingRanges,
}

pub struct TextDocument {
    identity: u64,
    content: TextDocumentContentState,
    history: TextDocumentHistoryState,
}

/// A live document's identity and text generation, independent of view-anchor COW.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DocumentRevision {
    identity: u64,
    generation: u64,
}

/// Validity of a history action or cache, including replacement documents and
/// history-only changes. Text generations alone can repeat during replay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct HistoryToken {
    document: DocumentRevision,
    history: u64,
}

fn next_document_identity() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

impl Clone for TextDocument {
    fn clone(&self) -> Self {
        // A clone is independently editable; replacing a buffer with it must
        // invalidate outstanding actions even if generations happen to match.
        Self {
            identity: next_document_identity(),
            content: self.content.clone(),
            history: self.history.clone(),
        }
    }
}

#[derive(Clone)]
pub(super) struct TextDocumentContentState {
    pub(super) piece_tree: Arc<PieceTreeLite>,
    pub(super) preferred_line_ending: LineEndingStyle,
}

impl TextDocumentContentState {
    fn new(text: String, preferred_line_ending: LineEndingStyle) -> Self {
        Self {
            piece_tree: Arc::new(PieceTreeLite::from_string(text)),
            preferred_line_ending,
        }
    }
}

#[derive(Clone)]
pub(super) struct TextDocumentHistoryState {
    pub(super) entries: Vec<PieceHistoryEntry>,
    pub(super) tree: super::edit_tree::EditTree,
    pub(super) byte_usage: usize,
    pub(super) next_id: u64,
    pub(super) revision_counter: u64,
    pub(super) budget: TextHistoryBudget,
    pub(super) latest_operation_record: Option<TextDocumentOperationRecord>,
    pub(super) latest_update_at: Option<Instant>,
    pub(super) pending_generation_before: Option<u32>,
}

impl Default for TextDocumentHistoryState {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            tree: super::edit_tree::EditTree::default(),
            byte_usage: 0,
            next_id: 1,
            revision_counter: 0,
            budget: TextHistoryBudget::default(),
            latest_operation_record: None,
            latest_update_at: None,
            pending_generation_before: None,
        }
    }
}

impl TextDocument {
    #[must_use]
    pub fn new(text: String) -> Self {
        Self::with_preferred_line_ending(text, platform_default_line_ending())
    }

    #[must_use]
    pub fn with_preferred_line_ending(
        text: String,
        preferred_line_ending: LineEndingStyle,
    ) -> Self {
        let mut document = Self {
            identity: next_document_identity(),
            content: TextDocumentContentState::new(text, preferred_line_ending),
            history: TextDocumentHistoryState::default(),
        };
        document.record_initial_content();
        document
    }

    pub(crate) fn from_utf8_file(
        path: &Path,
        file_offset: u64,
        sample_limit: usize,
    ) -> io::Result<(Self, String, usize)> {
        let (piece_tree, sample, line_count) =
            PieceTreeLite::from_utf8_file(path, file_offset, sample_limit)?;
        let mut document = Self {
            identity: next_document_identity(),
            content: TextDocumentContentState {
                piece_tree: Arc::new(piece_tree),
                preferred_line_ending: platform_default_line_ending(),
            },
            history: TextDocumentHistoryState::default(),
        };
        document.record_initial_content();
        Ok((document, sample, line_count))
    }

    /// Extract the full text content as a new String from the piece tree.
    #[must_use]
    pub fn extract_text(&self) -> String {
        let text = self.content.piece_tree.extract_text();
        capacity_metrics::record_full_text_flatten(text.len());
        text
    }

    #[must_use]
    pub fn piece_tree(&self) -> &PieceTreeLite {
        self.content.piece_tree.as_ref()
    }

    /// Mutable access to the underlying piece tree. Used by view code to
    /// create/release stable anchors. Triggers `Arc::make_mut`, which clones
    /// the tree if it is currently shared (e.g. by an undo snapshot) — that
    /// is the intended copy-on-write behavior; the view's anchors must live
    /// on the new clone, not the snapshot.
    pub fn piece_tree_mut(&mut self) -> &mut PieceTreeLite {
        Arc::make_mut(&mut self.content.piece_tree)
    }

    /// Identity plus text generation, without copying text or runtime anchors.
    #[must_use]
    pub(crate) fn revision_key(&self) -> DocumentRevision {
        DocumentRevision {
            identity: self.identity,
            generation: self.content.piece_tree.generation(),
        }
    }

    #[must_use]
    pub fn snapshot(&self) -> DocumentSnapshot {
        DocumentSnapshot::from_shared(self.content.piece_tree.clone())
    }

    /// Hash decoded text in bounded pieces, without a full-document allocation.
    pub(crate) fn content_digest(&self) -> String {
        let snapshot = self.snapshot();
        let mut hash = blake3::Hasher::new();
        for start in (0..snapshot.len_chars()).step_by(64 * 1024) {
            hash.update(
                snapshot
                    .extract_range(start..(start + 64 * 1024).min(snapshot.len_chars()))
                    .as_bytes(),
            );
        }
        hash.finalize().to_hex().to_string()
    }

    #[must_use]
    pub fn operation_undo_depth(&self) -> usize {
        self.history.tree.position()
    }

    #[must_use]
    pub fn operation_redo_depth(&self) -> usize {
        self.history.tree.route().len() - self.history.tree.position()
    }

    #[must_use]
    pub fn latest_operation_record(&self) -> Option<&TextDocumentOperationRecord> {
        self.history.latest_operation_record.as_ref()
    }

    pub fn clear_operation_history(&mut self) {
        let revision = self.history.revision_counter;
        self.history = TextDocumentHistoryState::default();
        self.history.revision_counter = revision.wrapping_add(1);
        self.record_initial_content();
    }

    #[must_use]
    pub fn history_entries(&self) -> &[PieceHistoryEntry] {
        &self.history.entries
    }

    #[must_use]
    pub fn history_revision_counter(&self) -> u64 {
        self.history.revision_counter
    }

    pub(crate) fn history_token(&self) -> HistoryToken {
        HistoryToken {
            document: self.revision_key(),
            history: self.history.revision_counter,
        }
    }

    fn advance_history_revision(&mut self) {
        self.history.revision_counter = self.history.revision_counter.wrapping_add(1);
    }

    #[must_use]
    pub fn history_byte_usage(&self) -> usize {
        self.history.byte_usage
    }

    #[must_use]
    pub fn oldest_history_global_seq(&self) -> Option<u64> {
        self.history.entries.first().map(|entry| entry.global_seq)
    }

    pub fn drop_oldest_history_entry(&mut self) -> Option<PieceHistoryEntry> {
        // Durable edits are never evicted to meet a memory budget.
        None
    }

    pub fn set_history_budget(&mut self, budget: TextHistoryBudget) {
        self.history.budget = budget.sanitized();
    }

    pub fn exported_history(&self) -> Vec<PersistedHistoryEntry> {
        self.history
            .entries
            .iter()
            .map(|entry| self.export_history_entry(entry))
            .collect()
    }

    pub fn restore_exported_history(&mut self, entries: Vec<PersistedHistoryEntry>) {
        if entries.is_empty() {
            return;
        }
        let now = entries.iter().find(|e| e.current).map_or(0, |e| e.id);
        let tip = entries.iter().find(|e| e.selected_tip).map_or(0, |e| e.id);
        let Some(tree) = super::edit_tree::EditTree::restore(
            entries.iter().map(|e| (e.id, e.parent_id)),
            now,
            tip,
        ) else {
            return;
        };
        let original_content = self.content.clone();
        self.content.piece_tree = Arc::new(PieceTreeLite::from_string(String::new()));
        self.history.entries.clear();
        self.history.byte_usage = 0;
        self.history.latest_update_at = None;
        self.history.tree = super::edit_tree::EditTree::restore(tree.links(), 0, tip).unwrap();
        for persisted in entries {
            let entry = self.import_history_entry(persisted);
            self.history.byte_usage += entry.byte_cost();
            self.history.entries.push(entry);
        }
        self.history.next_id = self.history.entries.last().map_or(1, |e| e.id + 1);
        if self.navigate_history(now).is_err() {
            self.content = original_content;
            self.history.tree = tree;
            for entry in &mut self.history.entries { entry.flags.replayable = false; }
        }
        self.refresh_tree_flags();
        self.advance_history_revision();
    }

    pub fn set_preferred_line_ending(&mut self, preferred_line_ending: LineEndingStyle) {
        self.content.preferred_line_ending = preferred_line_ending;
    }

    pub fn replace_text(&mut self, text: String) {
        self.identity = next_document_identity();
        self.content.piece_tree = Arc::new(PieceTreeLite::from_string(text));
        self.clear_operation_history();
    }

    pub(crate) fn replace_char_ranges_with_undo(
        &mut self,
        replacements: TextReplacements<'_>,
        previous_selection: CursorRange,
        next_selection: CursorRange,
    ) -> Result<(), TextReplacementError> {
        self.replace_char_ranges_with_source(
            replacements,
            previous_selection,
            next_selection,
            PieceSource::SearchReplace,
        )
    }

    pub(crate) fn replace_char_ranges_with_source(
        &mut self,
        replacements: TextReplacements<'_>,
        previous_selection: CursorRange,
        next_selection: CursorRange,
        source: PieceSource,
    ) -> Result<(), TextReplacementError> {
        if replacements.is_empty() {
            return Ok(());
        }

        validate_replacements(replacements, self.content.piece_tree.len_chars())?;
        self.capture_pending_history_generation_before();
        let mut operation_record = TextDocumentOperationRecord {
            previous_selection,
            next_selection,
            edits: Vec::with_capacity(replacements.len()),
        };
        for (range, replacement) in replacements {
            let deleted_text = self
                .content
                .piece_tree
                .extract_range_with_capacity(range.clone(), range.len());
            let deleted_spans = self.byte_spans_for_range(range.clone());
            let normalized = normalize_inserted_text_line_endings(
                replacement,
                self.content.preferred_line_ending,
            );
            self.delete_char_range_internal(range.clone());
            self.insert_raw_text_with_source(&normalized, range.start, source);
            operation_record.edits.push(TextDocumentEditOperation {
                start_char: range.start,
                deleted_text,
                inserted_text: normalized.into_owned(),
                deleted_spans,
            });
        }
        self.push_operation_record(operation_record, source);
        Ok(())
    }

    pub(crate) fn validate_char_replacements(
        &self,
        replacements: TextReplacements<'_>,
    ) -> Result<(), TextReplacementError> {
        validate_replacements(replacements, self.content.piece_tree.len_chars())
    }

    pub fn undo_last_operation(&mut self) -> Option<CursorRange> {
        self.replay_last_operation(OperationDirection::Undo)
    }

    pub fn redo_last_operation(&mut self) -> Option<CursorRange> {
        self.replay_last_operation(OperationDirection::Redo)
    }

    pub(crate) fn apply_text_history_undo(
        &mut self,
        entry_id: u64,
    ) -> Result<CursorRange, TextHistoryApplyError> {
        self.apply_text_history_entry(entry_id, OperationDirection::Undo)
    }

    pub(crate) fn apply_text_history_redo(
        &mut self,
        entry_id: u64,
    ) -> Result<CursorRange, TextHistoryApplyError> {
        self.apply_text_history_entry(entry_id, OperationDirection::Redo)
    }

    // --- Native editor direct mutation API ---

    #[must_use]
    pub fn preferred_line_ending_str(&self) -> &str {
        self.content.preferred_line_ending.as_str()
    }

    /// Insert text directly via piece tree.
    pub fn insert_direct(&mut self, char_index: usize, text: &str) {
        self.capture_pending_history_generation_before();
        self.insert_raw_text_with_source(text, char_index, PieceSource::Edit);
    }

    pub fn insert_direct_with_source(
        &mut self,
        char_index: usize,
        text: &str,
        source: PieceSource,
    ) {
        self.capture_pending_history_generation_before();
        self.insert_raw_text_with_source(text, char_index, source);
    }

    #[must_use]
    pub fn byte_spans_for_range(&self, char_range: Range<usize>) -> Vec<ByteSpan> {
        self.content
            .piece_tree
            .spans_for_range(char_range)
            .map(|span| span.byte_span)
            .collect()
    }

    /// Delete a char range directly via piece tree.
    pub fn delete_char_range_direct(&mut self, char_range: Range<usize>) {
        self.capture_pending_history_generation_before();
        self.delete_char_range_internal(char_range);
    }

    /// Push a native operation record for undo/redo.
    pub fn push_edit_operation(&mut self, record: OperationRecord) {
        self.push_edit_operation_with_source(record, PieceSource::Edit);
    }

    pub fn push_edit_operation_with_source(
        &mut self,
        record: OperationRecord,
        source: PieceSource,
    ) {
        let converted = TextDocumentOperationRecord {
            previous_selection: record.previous_cursor,
            next_selection: record.next_cursor,
            edits: record.edits,
        };
        self.push_operation_record(converted, source);
    }

    fn insert_raw_text(&mut self, text: &str, char_index: usize) {
        self.insert_raw_text_with_source(text, char_index, PieceSource::Edit);
    }

    fn insert_raw_text_with_source(&mut self, text: &str, char_index: usize, source: PieceSource) {
        Arc::make_mut(&mut self.content.piece_tree).insert_with_source(char_index, text, source);
    }

    fn delete_char_range_internal(&mut self, char_range: Range<usize>) {
        assert!(
            char_range.start <= char_range.end,
            "start must be <= end, but got {char_range:?}"
        );
        Arc::make_mut(&mut self.content.piece_tree).remove_char_range(char_range);
    }

    fn replace_char_range_raw(&mut self, char_range: Range<usize>, replacement: &str) {
        self.delete_char_range_internal(char_range.clone());
        self.insert_raw_text(replacement, char_range.start);
    }

    fn fingerprint_for_history_edits(&self, edits: &[PieceHistoryEdit]) -> u64 {
        // Persistence joins deleted spans into one payload. Hash logical payloads
        // rather than storage boundaries so recovery and compaction agree.
        fingerprint_parts(edits.iter().flat_map(|edit| {
            let deleted = match edit.deleted_spans() {
                [] => None,
                [span] => Some(self.content.piece_tree.text_for_span(*span)),
                spans => Some(super::PieceTreeText::owned(self.text_for_spans(spans))),
            };
            let inserted = edit
                .inserted_span()
                .map(|span| self.content.piece_tree.text_for_span(span));
            deleted.into_iter().chain(inserted)
        }))
    }

    fn visible_generation(&self) -> u32 {
        self.content
            .piece_tree
            .generation()
            .min(u64::from(u32::MAX)) as u32
    }

    fn capture_pending_history_generation_before(&mut self) {
        if self.history.pending_generation_before.is_none() {
            self.history.pending_generation_before = Some(self.visible_generation());
        }
    }
}

fn validate_replacements(
    replacements: TextReplacements<'_>,
    text_char_len: usize,
) -> Result<(), TextReplacementError> {
    let mut previous_start = text_char_len;

    for (range, _) in replacements {
        if range.start > range.end {
            return Err(TextReplacementError::InvalidRange);
        }
        if range.end > text_char_len {
            return Err(TextReplacementError::OutOfBounds);
        }
        if range.start > previous_start {
            return Err(TextReplacementError::NotDescending);
        }
        if range.end > previous_start {
            return Err(TextReplacementError::OverlappingRanges);
        }
        previous_start = range.start;
    }

    Ok(())
}

#[cfg(test)]
mod tests;
