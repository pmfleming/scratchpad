mod history_ops;
pub(crate) use history_ops::HistorySnapshot;
mod tree_ops;

use super::PersistentVector;
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
    StorageUnreadable,
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
    pub(super) entries: PersistentVector<PieceHistoryEntry>,
    pub(super) tree: super::edit_tree::EditTree,
    pub(super) byte_usage: usize,
    pub(super) next_id: u64,
    pub(super) revision_counter: u64,
    pub(super) budget: TextHistoryBudget,
    pub(super) latest_operation_record: Option<TextDocumentOperationRecord>,
    pub(super) latest_update_at: Option<Instant>,
}

impl Default for TextDocumentHistoryState {
    fn default() -> Self {
        Self {
            entries: PersistentVector::default(),
            tree: super::edit_tree::EditTree::default(),
            byte_usage: 0,
            next_id: 1,
            revision_counter: 0,
            budget: TextHistoryBudget::default(),
            latest_operation_record: None,
            latest_update_at: None,
        }
    }
}

impl TextDocument {
    pub fn bind_store(
        &mut self,
        store: crate::app::services::chunk_store::ChunkStoreHandle,
    ) -> io::Result<()> {
        self.piece_tree_mut().bind_store(store)
    }

    pub(crate) fn set_bulk_intake(&mut self, enabled: bool) {
        self.piece_tree_mut().set_bulk_intake(enabled);
    }
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
    pub fn try_extract_text(&self) -> io::Result<String> {
        let mut text = String::with_capacity(self.piece_tree().metrics().bytes);
        for span in self
            .piece_tree()
            .try_spans_for_range(0..self.piece_tree().len_chars())
        {
            text.push_str(&span?.text);
        }
        capacity_metrics::record_full_text_flatten(text.len());
        Ok(text)
    }

    /// Infallible access for callers that already hold a complete text pin.
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
    pub(crate) fn try_content_digest(&self) -> io::Result<String> {
        self.snapshot().try_content_digest()
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
    pub fn history_entries(&self) -> &PersistentVector<PieceHistoryEntry> {
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

    pub fn set_history_budget(&mut self, budget: TextHistoryBudget) {
        self.history.budget = budget.sanitized();
    }

    #[must_use]
    #[cfg(test)]
    pub fn exported_history(&self) -> Vec<PersistedHistoryEntry> {
        self.try_exported_history().unwrap()
    }

    pub fn try_exported_history(&self) -> io::Result<Vec<PersistedHistoryEntry>> {
        let snapshot = self.history_snapshot();
        (0..snapshot.len())
            .map(|index| snapshot.entry(index))
            .collect()
    }

    /// The repository validates each row before handing it to this streaming importer.
    pub(crate) fn import_history_row(&mut self, persisted: PersistedHistoryEntry) {
        let entry = self.import_history_entry(persisted);
        self.history.byte_usage += entry.byte_cost();
        self.history.next_id = entry.id.saturating_add(1);
        self.history.entries.push(entry);
    }

    pub(crate) fn finish_history_import(&mut self, now: u64, tip: u64) -> bool {
        let Some(tree) = super::edit_tree::EditTree::restore(
            self.history
                .entries
                .iter()
                .map(|entry| (entry.id, entry.parent_id)),
            0,
            tip,
        ) else {
            return false;
        };
        if !tree.route().contains(&now) && now != 0 {
            return false;
        }
        self.history.tree = tree;
        if self.navigate_history(now).is_err() {
            return false;
        }
        self.refresh_tree_flags();
        self.seal_history_group();
        true
    }

    pub fn restore_exported_history(&mut self, entries: Vec<PersistedHistoryEntry>) -> bool {
        if entries.is_empty() {
            return true;
        }
        if entries
            .iter()
            .any(|entry| entry.id == u64::MAX || !entry.payloads_valid())
            || entries.iter().filter(|entry| entry.current).count() > 1
            || entries.iter().filter(|entry| entry.selected_tip).count() > 1
        {
            return false;
        }
        let now = entries.iter().find(|e| e.current).map_or(0, |e| e.id);
        let tip = entries.iter().find(|e| e.selected_tip).map_or(0, |e| e.id);
        let Some(tree) = super::edit_tree::EditTree::restore(
            entries.iter().map(|e| (e.id, e.parent_id)),
            now,
            tip,
        ) else {
            return false;
        };
        let Some(root_tree) = super::edit_tree::EditTree::restore(tree.links(), 0, tip) else {
            return false;
        };
        let original_content = self.content.clone();
        self.content.piece_tree = Arc::new(PieceTreeLite::from_string(String::new()));
        let original_history = std::mem::take(&mut self.history);
        self.history.budget = original_history.budget;
        self.history.byte_usage = 0;
        self.history.latest_update_at = None;
        self.history.tree = root_tree;
        for persisted in entries {
            let entry = self.import_history_entry(persisted);
            self.history.byte_usage += entry.byte_cost();
            self.history.entries.push(entry);
        }
        self.history.next_id = self.history.entries.last().map_or(1, |e| e.id + 1);
        if self.navigate_history(now).is_err() {
            self.content = original_content;
            self.history = original_history;
            return false;
        }
        self.refresh_tree_flags();
        self.identity = next_document_identity();
        self.advance_history_revision();
        true
    }

    pub fn set_preferred_line_ending(&mut self, preferred_line_ending: LineEndingStyle) {
        self.content.preferred_line_ending = preferred_line_ending;
    }

    pub fn replace_text(&mut self, text: String) {
        let store = self.piece_tree().store().cloned();
        self.identity = next_document_identity();
        self.content.piece_tree = Arc::new(PieceTreeLite::from_string(text));
        if let Some(store) = store
            && let Err(cause) = self.bind_store(store)
        {
            log::warn!("Text binding failed after replacement; text remains in RAM: {cause}");
        }
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
        if self.piece_tree().read_fault().is_some() {
            return Err(TextReplacementError::StorageUnreadable);
        }
        let _pins = replacements
            .iter()
            .map(|(range, _)| self.piece_tree().pin_range(range.clone()))
            .collect::<io::Result<Vec<_>>>()
            .map_err(|_| TextReplacementError::StorageUnreadable)?;
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

    // --- Native editor direct mutation API ---

    #[must_use]
    pub fn preferred_line_ending_str(&self) -> &str {
        self.content.preferred_line_ending.as_str()
    }

    /// Insert text directly via piece tree.
    pub fn insert_direct(&mut self, char_index: usize, text: &str) {
        self.insert_raw_text_with_source(text, char_index, PieceSource::Edit);
    }

    pub fn insert_direct_with_source(
        &mut self,
        char_index: usize,
        text: &str,
        source: PieceSource,
    ) {
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

    fn insert_raw_text_with_source(&mut self, text: &str, char_index: usize, source: PieceSource) {
        if self.piece_tree().read_fault().is_some() {
            return;
        }
        let Ok(_pin) = self.piece_tree().pin_range(char_index..char_index) else {
            return;
        };
        Arc::make_mut(&mut self.content.piece_tree).insert_with_source(char_index, text, source);
    }

    fn delete_char_range_internal(&mut self, char_range: Range<usize>) {
        if self.piece_tree().read_fault().is_some() {
            return;
        }
        let Ok(_pin) = self.piece_tree().pin_range(char_range.clone()) else {
            return;
        };
        assert!(
            char_range.start <= char_range.end,
            "start must be <= end, but got {char_range:?}"
        );
        Arc::make_mut(&mut self.content.piece_tree).remove_char_range(char_range);
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

    #[cfg(test)]
    fn visible_generation(&self) -> u64 {
        self.content.piece_tree.generation()
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
