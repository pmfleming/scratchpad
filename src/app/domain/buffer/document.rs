mod history_ops;

use super::history::{OperationDirection, TextDocumentEditOperation, TextHistoryApplyError};
use super::{
    ByteSpan, DocumentSnapshot, LineEndingStyle, PersistedHistoryEntry, PieceHistoryEdit,
    PieceHistoryEntry, PieceSource, PieceTreeLite, TextDocumentOperationRecord, TextHistoryBudget,
    fingerprint_parts, normalize_inserted_text_line_endings, platform_default_line_ending,
};
use crate::app::capacity_metrics;
use crate::app::ui::editor_content::native_editor::{CursorRange, OperationRecord};
use std::borrow::Cow;
use std::ops::Range;
use std::sync::Arc;
use std::time::Instant;

pub(crate) type TextReplacements<'a> = &'a [(Range<usize>, String)];

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
    history: Vec<PieceHistoryEntry>,
    history_byte_usage: usize,
    next_history_id: u64,
    revision_counter: u64,
    history_budget: TextHistoryBudget,
    latest_operation_record: Option<TextDocumentOperationRecord>,
    latest_history_update_at: Option<Instant>,
    pending_history_generation_before: Option<u32>,
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
            history: Vec::new(),
            history_byte_usage: 0,
            next_history_id: 1,
            revision_counter: 0,
            history_budget: TextHistoryBudget::default(),
            latest_operation_record: None,
            latest_history_update_at: None,
            pending_history_generation_before: None,
            preferred_line_ending,
        }
    }

    /// Extract the full text content as a new String from the piece tree.
    pub fn extract_text(&self) -> String {
        let text = self.piece_tree.extract_text();
        capacity_metrics::record_full_text_flatten(text.len());
        text
    }

    pub fn text_cow(&self) -> Cow<'_, str> {
        self.piece_tree
            .borrow_range(0..self.piece_tree.len_chars())
            .map(Cow::Borrowed)
            .unwrap_or_else(|| {
                let text = self.piece_tree.extract_text();
                capacity_metrics::record_full_text_flatten(text.len());
                Cow::Owned(text)
            })
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
        self.history
            .iter()
            .filter(|entry| !entry.is_undone())
            .count()
    }

    pub fn operation_redo_depth(&self) -> usize {
        self.history
            .iter()
            .filter(|entry| entry.is_undone())
            .count()
    }

    pub fn latest_operation_record(&self) -> Option<&TextDocumentOperationRecord> {
        self.latest_operation_record.as_ref()
    }

    pub fn clear_operation_history(&mut self) {
        self.history.clear();
        self.history_byte_usage = 0;
        self.latest_operation_record = None;
        self.latest_history_update_at = None;
        self.pending_history_generation_before = None;
        self.revision_counter = self.revision_counter.wrapping_add(1);
    }

    pub fn history_entries(&self) -> &[PieceHistoryEntry] {
        &self.history
    }

    pub fn history_revision_counter(&self) -> u64 {
        self.revision_counter
    }

    pub fn history_byte_usage(&self) -> usize {
        self.history_byte_usage
    }

    pub fn oldest_history_global_seq(&self) -> Option<u64> {
        self.history.first().map(|entry| entry.global_seq)
    }

    pub fn drop_oldest_history_entry(&mut self) -> Option<PieceHistoryEntry> {
        if self.history.is_empty() {
            None
        } else {
            self.revision_counter = self.revision_counter.wrapping_add(1);
            let removed = self.history.remove(0);
            self.history_byte_usage = self.history_byte_usage.saturating_sub(removed.byte_cost());
            self.compact_history_storage();
            Some(removed)
        }
    }

    pub fn set_history_budget(&mut self, budget: TextHistoryBudget) {
        self.history_budget = budget.sanitized();
        self.enforce_history_budget();
    }

    pub fn exported_history(&self) -> Vec<PersistedHistoryEntry> {
        let mut entries = self
            .history
            .iter()
            .map(|entry| self.export_history_entry(entry))
            .collect::<Vec<_>>();
        let mut payload_bytes = entries
            .iter()
            .map(PersistedHistoryEntry::payload_bytes)
            .sum::<usize>();
        let budget = self.history_budget.persisted_payload_budget as usize;
        for entry in &mut entries {
            if payload_bytes <= budget {
                break;
            }
            payload_bytes = payload_bytes.saturating_sub(entry.payload_bytes());
            entry.drop_payloads();
        }
        entries
    }

    pub fn restore_exported_history(&mut self, entries: Vec<PersistedHistoryEntry>) {
        self.history.clear();
        self.history_byte_usage = 0;
        let mut max_id = 0_u64;
        for persisted in entries {
            max_id = max_id.max(persisted.id);
            let entry = self.import_history_entry(persisted);
            self.history_byte_usage += entry.byte_cost();
            self.history.push(entry);
        }
        self.normalize_imported_redo_state();
        self.next_history_id = max_id.saturating_add(1).max(1);
        self.revision_counter = self.revision_counter.wrapping_add(1);
        self.enforce_history_budget();
    }

    pub fn revalidate_history_for_current_text(&mut self) {
        for index in 0..self.history.len() {
            let fingerprint = self.fingerprint_for_history_edits(&self.history[index].edits);
            self.history[index].flags.replayable &= fingerprint == self.history[index].fingerprint;
        }
        self.revision_counter = self.revision_counter.wrapping_add(1);
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

        validate_replacements(replacements, self.piece_tree.len_chars())?;
        self.capture_pending_history_generation_before();

        let mut operation_record = TextDocumentOperationRecord {
            previous_selection,
            next_selection,
            edits: Vec::with_capacity(replacements.len()),
        };
        for (range, replacement) in replacements {
            let deleted_text = self.piece_tree.extract_range(range.clone());
            let deleted_spans = self.byte_spans_for_range(range.clone());
            let normalized =
                normalize_inserted_text_line_endings(replacement, self.preferred_line_ending)
                    .into_owned();
            self.delete_char_range_internal(range.clone());
            self.insert_raw_text_with_source(&normalized, range.start, source);
            operation_record.edits.push(TextDocumentEditOperation {
                start_char: range.start,
                deleted_text,
                inserted_text: normalized,
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
        validate_replacements(replacements, self.piece_tree.len_chars())
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

    pub fn preferred_line_ending_str(&self) -> &str {
        self.preferred_line_ending.as_str()
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

    pub fn byte_spans_for_range(&self, char_range: Range<usize>) -> Vec<ByteSpan> {
        self.piece_tree
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
            edits: record
                .edits
                .into_iter()
                .map(|edit| TextDocumentEditOperation {
                    start_char: edit.start_char,
                    deleted_text: edit.deleted_text,
                    inserted_text: edit.inserted_text,
                    deleted_spans: edit.deleted_spans,
                })
                .collect(),
        };
        self.push_operation_record(converted, source);
    }

    fn insert_raw_text(&mut self, text: &str, char_index: usize) {
        self.insert_raw_text_with_source(text, char_index, PieceSource::Edit);
    }

    fn insert_raw_text_with_source(&mut self, text: &str, char_index: usize, source: PieceSource) {
        Arc::make_mut(&mut self.piece_tree).insert_with_source(char_index, text, source);
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

    fn fingerprint_for_history_edits(&self, edits: &[PieceHistoryEdit]) -> u64 {
        fingerprint_parts(
            edits
                .iter()
                .flat_map(PieceHistoryEdit::spans)
                .map(|span| self.piece_tree.text_for_span(span)),
        )
    }

    fn visible_generation(&self) -> u32 {
        self.piece_tree.generation().min(u32::MAX as u64) as u32
    }

    fn capture_pending_history_generation_before(&mut self) {
        if self.pending_history_generation_before.is_none() {
            self.pending_history_generation_before = Some(self.visible_generation());
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
