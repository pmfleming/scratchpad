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
use std::io;
use std::ops::Range;
use std::path::Path;
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
    content: TextDocumentContentState,
    history: TextDocumentHistoryState,
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
    pub(super) byte_usage: usize,
    pub(super) undo_depth: usize,
    pub(super) redo_depth: usize,
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
            byte_usage: 0,
            undo_depth: 0,
            redo_depth: 0,
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
        Self {
            content: TextDocumentContentState::new(text, preferred_line_ending),
            history: TextDocumentHistoryState::default(),
        }
    }

    pub(crate) fn from_utf8_file(
        path: &Path,
        file_offset: u64,
        sample_limit: usize,
    ) -> io::Result<(Self, String, usize)> {
        let (piece_tree, sample, line_count) =
            PieceTreeLite::from_utf8_file(path, file_offset, sample_limit)?;
        Ok((
            Self {
                content: TextDocumentContentState {
                    piece_tree: Arc::new(piece_tree),
                    preferred_line_ending: platform_default_line_ending(),
                },
                history: TextDocumentHistoryState::default(),
            },
            sample,
            line_count,
        ))
    }

    /// Extract the full text content as a new String from the piece tree.
    #[must_use]
    pub fn extract_text(&self) -> String {
        let text = self.content.piece_tree.extract_text();
        capacity_metrics::record_full_text_flatten(text.len());
        text
    }

    pub fn text_cow(&self) -> Cow<'_, str> {
        self.content
            .piece_tree
            .borrow_range(0..self.content.piece_tree.len_chars())
            .map_or_else(
                || {
                    let text = self.content.piece_tree.extract_text();
                    capacity_metrics::record_full_text_flatten(text.len());
                    Cow::Owned(text)
                },
                Cow::Borrowed,
            )
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

    #[must_use]
    pub fn snapshot(&self) -> DocumentSnapshot {
        DocumentSnapshot::from_shared(self.content.piece_tree.clone())
    }

    #[must_use]
    pub fn operation_undo_depth(&self) -> usize {
        self.history.undo_depth
    }

    #[must_use]
    pub fn operation_redo_depth(&self) -> usize {
        self.history.redo_depth
    }

    #[must_use]
    pub fn latest_operation_record(&self) -> Option<&TextDocumentOperationRecord> {
        self.history.latest_operation_record.as_ref()
    }

    pub fn clear_operation_history(&mut self) {
        self.history.entries.clear();
        self.history.byte_usage = 0;
        self.history.undo_depth = 0;
        self.history.redo_depth = 0;
        self.history.latest_operation_record = None;
        self.history.latest_update_at = None;
        self.history.pending_generation_before = None;
        self.history.revision_counter = self.history.revision_counter.wrapping_add(1);
    }

    #[must_use]
    pub fn history_entries(&self) -> &[PieceHistoryEntry] {
        &self.history.entries
    }

    #[must_use]
    pub fn history_revision_counter(&self) -> u64 {
        self.history.revision_counter
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
        if self.history.entries.is_empty() {
            None
        } else {
            self.history.revision_counter = self.history.revision_counter.wrapping_add(1);
            let removed = self.history.entries.remove(0);
            self.history.byte_usage = self.history.byte_usage.saturating_sub(removed.byte_cost());
            self.remove_history_depth(&removed);
            self.compact_history_storage();
            Some(removed)
        }
    }

    pub fn set_history_budget(&mut self, budget: TextHistoryBudget) {
        self.history.budget = budget.sanitized();
        self.enforce_history_budget();
    }

    pub fn exported_history(&self) -> Vec<PersistedHistoryEntry> {
        let mut entries = self
            .history
            .entries
            .iter()
            .map(|entry| self.export_history_entry(entry))
            .collect::<Vec<_>>();
        let mut payload_bytes = entries
            .iter()
            .map(PersistedHistoryEntry::payload_bytes)
            .sum::<usize>();
        let budget = self.history.budget.persisted_payload_budget as usize;
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
        self.history.entries.clear();
        self.history.byte_usage = 0;
        self.history.undo_depth = 0;
        self.history.redo_depth = 0;
        let mut max_id = 0_u64;
        for persisted in entries {
            max_id = max_id.max(persisted.id);
            let entry = self.import_history_entry(persisted);
            self.history.byte_usage += entry.byte_cost();
            self.history.entries.push(entry);
        }
        self.normalize_imported_redo_state();
        self.refresh_history_depths();
        self.history.next_id = max_id.saturating_add(1).max(1);
        self.history.revision_counter = self.history.revision_counter.wrapping_add(1);
        self.enforce_history_budget();
    }

    pub fn revalidate_history_for_current_text(&mut self) {
        for index in 0..self.history.entries.len() {
            let fingerprint =
                self.fingerprint_for_history_edits(&self.history.entries[index].edits);
            self.history.entries[index].flags.replayable &=
                fingerprint == self.history.entries[index].fingerprint;
        }
        self.history.revision_counter = self.history.revision_counter.wrapping_add(1);
    }

    pub fn set_preferred_line_ending(&mut self, preferred_line_ending: LineEndingStyle) {
        self.content.preferred_line_ending = preferred_line_ending;
    }

    pub fn replace_text(&mut self, text: String) {
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
        fingerprint_parts(
            edits
                .iter()
                .flat_map(PieceHistoryEdit::spans)
                .map(|span| self.content.piece_tree.text_for_span(span)),
        )
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

    fn add_history_depth(&mut self, entry: &PieceHistoryEntry) {
        if entry.is_undone() {
            self.history.redo_depth = self.history.redo_depth.saturating_add(1);
        } else {
            self.history.undo_depth = self.history.undo_depth.saturating_add(1);
        }
    }

    fn remove_history_depth(&mut self, entry: &PieceHistoryEntry) {
        if entry.is_undone() {
            self.history.redo_depth = self.history.redo_depth.saturating_sub(1);
        } else {
            self.history.undo_depth = self.history.undo_depth.saturating_sub(1);
        }
    }

    fn mark_history_entry_undone(&mut self, index: usize, undone: bool) {
        let was_undone = self.history.entries[index].is_undone();
        if was_undone == undone {
            return;
        }
        if undone {
            self.history.undo_depth = self.history.undo_depth.saturating_sub(1);
            self.history.redo_depth = self.history.redo_depth.saturating_add(1);
        } else {
            self.history.redo_depth = self.history.redo_depth.saturating_sub(1);
            self.history.undo_depth = self.history.undo_depth.saturating_add(1);
        }
        self.history.entries[index].flags.undone = undone;
    }

    fn refresh_history_depths(&mut self) {
        self.history.undo_depth = 0;
        self.history.redo_depth = 0;
        for entry in &self.history.entries {
            if entry.is_undone() {
                self.history.redo_depth += 1;
            } else {
                self.history.undo_depth += 1;
            }
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
