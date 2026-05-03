use super::{
    ByteSpan, DocumentSnapshot, LineEndingStyle, PersistedCursorRange, PersistedHistoryEdit,
    PersistedHistoryEntry, PieceHistoryEdit, PieceHistoryEdits, PieceHistoryEntry,
    PieceHistoryFlags, PieceSource, PieceTreeLite, TEXT_HISTORY_COALESCE_WINDOW, TextHistoryBudget,
    fingerprint_parts, platform_default_line_ending,
};
use crate::app::capacity_metrics;
use crate::app::ui::editor_content::native_editor::{CharCursor, CursorRange, OperationRecord};
use std::borrow::Cow;
use std::ops::Range;
use std::sync::Arc;
use std::time::Instant;

/// After a soft divider (`,` `;` `:`) the entry is sealed only if the next
/// keystroke arrives later than this. Inside this window the entry keeps
/// growing past the soft boundary so a continuous typing burst stays one entry.
const TEXT_HISTORY_SOFT_DIVIDER_PAUSE: std::time::Duration =
    std::time::Duration::from_millis(400);

fn is_hard_divider(ch: char) -> bool {
    matches!(ch, '.' | '?' | '!' | '\n' | '\r')
}

fn is_soft_divider(ch: char) -> bool {
    matches!(ch, ',' | ';' | ':' | '-')
}

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
    pub deleted_spans: Vec<ByteSpan>,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TextHistoryApplyError {
    OutOfBounds,
    Conflict,
}

#[derive(Clone)]
pub struct TextDocument {
    piece_tree: Arc<PieceTreeLite>,
    history: Vec<PieceHistoryEntry>,
    next_history_id: u64,
    revision_counter: u64,
    history_budget: TextHistoryBudget,
    latest_operation_record: Option<TextDocumentOperationRecord>,
    latest_history_update_at: Option<Instant>,
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
            next_history_id: 1,
            revision_counter: 0,
            history_budget: TextHistoryBudget::default(),
            latest_operation_record: None,
            latest_history_update_at: None,
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
        self.latest_operation_record = None;
        self.latest_history_update_at = None;
        self.revision_counter = self.revision_counter.wrapping_add(1);
    }

    pub fn history_entries(&self) -> &[PieceHistoryEntry] {
        &self.history
    }

    pub fn history_revision_counter(&self) -> u64 {
        self.revision_counter
    }

    pub fn history_byte_usage(&self) -> usize {
        self.history.iter().map(PieceHistoryEntry::byte_cost).sum()
    }

    pub fn oldest_history_seq(&self) -> Option<u64> {
        self.history.first().map(|entry| entry.seq)
    }

    pub fn drop_oldest_history_entry(&mut self) -> Option<PieceHistoryEntry> {
        if self.history.is_empty() {
            None
        } else {
            self.revision_counter = self.revision_counter.wrapping_add(1);
            let removed = self.history.remove(0);
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
        let mut max_id = 0;
        for persisted in entries {
            max_id = max_id.max(persisted.id);
            let entry = self.import_history_entry(persisted);
            self.history.push(entry);
        }
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

        let mut operation_record = TextDocumentOperationRecord {
            previous_selection,
            next_selection,
            edits: Vec::with_capacity(replacements.len()),
        };
        for (range, replacement) in replacements {
            let deleted_text = self.piece_tree.extract_range(range.clone());
            let deleted_spans = self.byte_spans_for_range(range.clone());
            let normalized =
                normalize_editor_inserted_text(replacement, self.preferred_line_ending)
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

    pub fn byte_spans_for_range(&self, char_range: Range<usize>) -> Vec<ByteSpan> {
        self.piece_tree
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

    fn insert_raw_text(&mut self, text: &str, char_index: usize) -> usize {
        self.insert_raw_text_with_source(text, char_index, PieceSource::Edit)
    }

    fn insert_raw_text_with_source(
        &mut self,
        text: &str,
        char_index: usize,
        source: PieceSource,
    ) -> usize {
        Arc::make_mut(&mut self.piece_tree).insert_with_source(char_index, text, source);
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

    fn push_operation_record(&mut self, record: TextDocumentOperationRecord, source: PieceSource) {
        self.latest_operation_record = Some(record.clone());
        self.history.retain(|entry| !entry.is_undone());
        if self.try_coalesce_history(&record, source) {
            self.revision_counter = self.revision_counter.wrapping_add(1);
            return;
        }

        let entry = self.history_entry_from_operation(record, source);
        self.history.push(entry);
        self.revision_counter = self.revision_counter.wrapping_add(1);
        self.enforce_history_budget();
    }

    fn replay_last_operation(&mut self, direction: OperationDirection) -> Option<CursorRange> {
        let entry_id = match direction {
            OperationDirection::Undo => self
                .history
                .iter()
                .rev()
                .find(|entry| !entry.is_undone() && entry.flags.replayable)
                .map(|entry| entry.id)?,
            OperationDirection::Redo => self
                .history
                .iter()
                .find(|entry| entry.is_undone() && entry.flags.replayable)
                .map(|entry| entry.id)?,
        };
        self.apply_text_history_entry(entry_id, direction).ok()
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

    fn apply_text_history_entry(
        &mut self,
        entry_id: u64,
        direction: OperationDirection,
    ) -> Result<CursorRange, TextHistoryApplyError> {
        let index = self
            .history
            .iter()
            .position(|entry| entry.id == entry_id)
            .ok_or(TextHistoryApplyError::OutOfBounds)?;
        let indices = match direction {
            OperationDirection::Undo => {
                if self.history[index].is_undone() {
                    return Err(TextHistoryApplyError::Conflict);
                }
                (index..self.history.len())
                    .rev()
                    .filter(|idx| !self.history[*idx].is_undone())
                    .collect::<Vec<_>>()
            }
            OperationDirection::Redo => {
                if !self.history[index].is_undone() {
                    return Err(TextHistoryApplyError::Conflict);
                }
                (0..=index)
                    .filter(|idx| self.history[*idx].is_undone())
                    .collect::<Vec<_>>()
            }
        };

        let mut applied_selection = None;
        for idx in indices {
            if !self.history[idx].flags.replayable {
                return Err(TextHistoryApplyError::Conflict);
            }
            let record = self.operation_from_history_entry(&self.history[idx]);
            self.validate_text_history_record(&record, direction)?;
            self.apply_operation_record(&record, direction);
            self.history[idx].flags.undone = matches!(direction, OperationDirection::Undo);
            self.latest_operation_record = Some(record.clone());
            applied_selection = Some(direction.selection(&record));
        }
        self.revision_counter = self.revision_counter.wrapping_add(1);
        applied_selection.ok_or(TextHistoryApplyError::Conflict)
    }

    fn validate_text_history_record(
        &self,
        record: &TextDocumentOperationRecord,
        direction: OperationDirection,
    ) -> Result<(), TextHistoryApplyError> {
        let expected_generation = self
            .history
            .iter()
            .find(|entry| {
                let entry_record = self.operation_from_history_entry(entry);
                entry_record == *record
            })
            .map(|entry| match direction {
                OperationDirection::Undo => entry.visible_generation_after,
                OperationDirection::Redo => entry.visible_generation_before,
            });
        if expected_generation == Some(self.piece_tree.generation().min(u32::MAX as u64) as u32) {
            return Ok(());
        }

        let expected_parts = record_expected_parts(record, direction);
        let expected_fingerprint = fingerprint_parts(expected_parts.iter().map(String::as_str));
        let current_fingerprint = fingerprint_parts(
            record_current_parts(self.piece_tree.as_ref(), record, direction)?
                .iter()
                .map(String::as_str),
        );
        if expected_fingerprint == current_fingerprint {
            return Ok(());
        }

        for edit in &record.edits {
            let (expected, replaced_len) = match direction {
                OperationDirection::Undo => (
                    edit.inserted_text.as_str(),
                    edit.inserted_text.chars().count(),
                ),
                OperationDirection::Redo => (
                    edit.deleted_text.as_str(),
                    edit.deleted_text.chars().count(),
                ),
            };
            let range = edit.start_char..edit.start_char + replaced_len;
            if range.end > self.piece_tree.len_chars() {
                return Err(TextHistoryApplyError::OutOfBounds);
            }
            if !expected.is_empty() && self.piece_tree.extract_range(range) != expected {
                return Err(TextHistoryApplyError::Conflict);
            }
        }
        Ok(())
    }

    fn export_history_entry(&self, entry: &PieceHistoryEntry) -> PersistedHistoryEntry {
        PersistedHistoryEntry {
            id: entry.id,
            seq: entry.seq,
            source: entry.source,
            visible_generation_before: entry.visible_generation_before,
            visible_generation_after: entry.visible_generation_after,
            fingerprint: entry.fingerprint,
            summary: entry.summary.clone(),
            flags: entry.flags,
            previous_selection: persist_cursor_range(entry.previous_selection),
            next_selection: persist_cursor_range(entry.next_selection),
            edits: entry
                .edits
                .iter()
                .map(|edit| self.export_history_edit(edit))
                .collect(),
        }
    }

    fn export_history_edit(&self, edit: &PieceHistoryEdit) -> PersistedHistoryEdit {
        match edit {
            PieceHistoryEdit::Inserted { start_char, span } => {
                let text = self.piece_tree.text_for_span(*span).to_owned();
                PersistedHistoryEdit::Inserted {
                    start_char: *start_char,
                    inserted_len: text.chars().count().min(u32::MAX as usize) as u32,
                    inserted_payload: Some(text),
                }
            }
            PieceHistoryEdit::Deleted { start_char, spans } => {
                let text = self.text_for_spans(spans);
                PersistedHistoryEdit::Deleted {
                    start_char: *start_char,
                    deleted_len: text.chars().count().min(u32::MAX as usize) as u32,
                    deleted_payload: Some(text),
                }
            }
            PieceHistoryEdit::Replaced {
                start_char,
                deleted,
                inserted,
            } => {
                let deleted_text = self.text_for_spans(deleted);
                let inserted_text = self.piece_tree.text_for_span(*inserted).to_owned();
                PersistedHistoryEdit::Replaced {
                    start_char: *start_char,
                    deleted_len: deleted_text.chars().count().min(u32::MAX as usize) as u32,
                    inserted_len: inserted_text.chars().count().min(u32::MAX as usize) as u32,
                    deleted_payload: Some(deleted_text),
                    inserted_payload: Some(inserted_text),
                }
            }
        }
    }

    fn import_history_entry(&mut self, persisted: PersistedHistoryEntry) -> PieceHistoryEntry {
        let all_payloads = persisted.has_all_payloads();
        let edits = persisted
            .edits
            .into_iter()
            .map(|edit| self.import_history_edit(edit, persisted.source))
            .collect::<PieceHistoryEdits>();
        let restored_fingerprint = self.fingerprint_for_history_edits(&edits);
        let mut flags = persisted.flags;
        flags.replayable &= all_payloads && restored_fingerprint == persisted.fingerprint;
        PieceHistoryEntry {
            id: persisted.id,
            seq: persisted.seq,
            source: persisted.source,
            visible_generation_before: persisted.visible_generation_before,
            visible_generation_after: persisted.visible_generation_after,
            fingerprint: persisted.fingerprint,
            summary: persisted.summary,
            edits,
            flags,
            previous_selection: restore_cursor_range(persisted.previous_selection),
            next_selection: restore_cursor_range(persisted.next_selection),
        }
    }

    fn import_history_edit(
        &mut self,
        edit: PersistedHistoryEdit,
        source: PieceSource,
    ) -> PieceHistoryEdit {
        let empty = || ByteSpan {
            buffer: super::piece_tree::PieceBuffer::Add,
            start_byte: 0,
            byte_len: 0,
        };
        let tree = Arc::make_mut(&mut self.piece_tree);
        match edit {
            PersistedHistoryEdit::Inserted {
                start_char,
                inserted_payload,
                ..
            } => PieceHistoryEdit::Inserted {
                start_char,
                span: inserted_payload
                    .as_deref()
                    .map(|text| tree.append_history_text(text, source))
                    .unwrap_or_else(empty),
            },
            PersistedHistoryEdit::Deleted {
                start_char,
                deleted_payload,
                ..
            } => PieceHistoryEdit::Deleted {
                start_char,
                spans: deleted_payload
                    .as_deref()
                    .map(|text| tree.append_history_text(text, source))
                    .map(|span| vec![span])
                    .unwrap_or_default(),
            },
            PersistedHistoryEdit::Replaced {
                start_char,
                deleted_payload,
                inserted_payload,
                ..
            } => PieceHistoryEdit::Replaced {
                start_char,
                deleted: deleted_payload
                    .as_deref()
                    .map(|text| tree.append_history_text(text, source))
                    .map(|span| vec![span])
                    .unwrap_or_default(),
                inserted: inserted_payload
                    .as_deref()
                    .map(|text| tree.append_history_text(text, source))
                    .unwrap_or_else(empty),
            },
        }
    }

    fn history_entry_from_operation(
        &mut self,
        record: TextDocumentOperationRecord,
        source: PieceSource,
    ) -> PieceHistoryEntry {
        let generation_after = self.piece_tree.generation().min(u32::MAX as u64) as u32;
        let mutation_count: u32 = record
            .edits
            .iter()
            .map(|edit| {
                u32::from(!edit.deleted_text.is_empty()) + u32::from(!edit.inserted_text.is_empty())
            })
            .sum::<u32>()
            .max(1);
        let generation_before = generation_after.saturating_sub(mutation_count);
        let edits = record
            .edits
            .iter()
            .map(|edit| self.history_edit_from_operation_edit(edit, source))
            .collect::<PieceHistoryEdits>();
        let fingerprint = self.fingerprint_for_history_edits(&edits);
        self.latest_history_update_at = Some(Instant::now());
        let entry = PieceHistoryEntry {
            id: self.next_history_id,
            seq: self.next_history_id,
            source,
            visible_generation_before: generation_before,
            visible_generation_after: generation_after,
            fingerprint,
            summary: operation_summary(source, &record),
            edits,
            flags: PieceHistoryFlags {
                undone: false,
                replayable: true,
                persisted: false,
            },
            previous_selection: record.previous_selection,
            next_selection: record.next_selection,
        };
        self.next_history_id = self.next_history_id.saturating_add(1);
        entry
    }

    fn history_edit_from_operation_edit(
        &mut self,
        edit: &TextDocumentEditOperation,
        source: PieceSource,
    ) -> PieceHistoryEdit {
        let tree = Arc::make_mut(&mut self.piece_tree);
        let start_char = edit.start_char.min(u32::MAX as usize) as u32;
        match (edit.deleted_text.is_empty(), edit.inserted_text.is_empty()) {
            (true, false) => PieceHistoryEdit::Inserted {
                start_char,
                span: tree.append_history_text(&edit.inserted_text, source),
            },
            (false, true) => PieceHistoryEdit::Deleted {
                start_char,
                spans: deleted_spans_or_payload(tree, edit, source),
            },
            (false, false) => PieceHistoryEdit::Replaced {
                start_char,
                deleted: deleted_spans_or_payload(tree, edit, source),
                inserted: tree.append_history_text(&edit.inserted_text, source),
            },
            (true, true) => PieceHistoryEdit::Inserted {
                start_char,
                span: ByteSpan {
                    buffer: super::piece_tree::PieceBuffer::Add,
                    start_byte: 0,
                    byte_len: 0,
                },
            },
        }
    }

    fn operation_from_history_entry(
        &self,
        entry: &PieceHistoryEntry,
    ) -> TextDocumentOperationRecord {
        TextDocumentOperationRecord {
            previous_selection: entry.previous_selection,
            next_selection: entry.next_selection,
            edits: entry
                .edits
                .iter()
                .map(|edit| match edit {
                    PieceHistoryEdit::Inserted { start_char, span } => TextDocumentEditOperation {
                        start_char: *start_char as usize,
                        deleted_text: String::new(),
                        inserted_text: self.piece_tree.text_for_span(*span).to_owned(),
                        deleted_spans: Vec::new(),
                    },
                    PieceHistoryEdit::Deleted { start_char, spans } => TextDocumentEditOperation {
                        start_char: *start_char as usize,
                        deleted_text: self.text_for_spans(spans),
                        inserted_text: String::new(),
                        deleted_spans: spans.clone(),
                    },
                    PieceHistoryEdit::Replaced {
                        start_char,
                        deleted,
                        inserted,
                    } => TextDocumentEditOperation {
                        start_char: *start_char as usize,
                        deleted_text: self.text_for_spans(deleted),
                        inserted_text: self.piece_tree.text_for_span(*inserted).to_owned(),
                        deleted_spans: deleted.clone(),
                    },
                })
                .collect(),
        }
    }

    fn text_for_spans(&self, spans: &[ByteSpan]) -> String {
        let mut text = String::new();
        for span in spans {
            text.push_str(self.piece_tree.text_for_span(*span));
        }
        text
    }

    fn try_coalesce_history(
        &mut self,
        incoming: &TextDocumentOperationRecord,
        source: PieceSource,
    ) -> bool {
        if source != PieceSource::Edit {
            return false;
        }
        let Some(latest_index) = self.history.len().checked_sub(1) else {
            return false;
        };
        let latest = &self.history[latest_index];
        let now = Instant::now();
        let elapsed = self
            .latest_history_update_at
            .map(|updated_at| now.duration_since(updated_at));
        if latest.source != PieceSource::Edit
            || latest.is_undone()
            || elapsed.is_none_or(|d| d > TEXT_HISTORY_COALESCE_WINDOW)
        {
            return false;
        }
        let latest_record = self.operation_from_history_entry(latest);
        if entry_sealed_by_divider(&latest_record, elapsed) {
            return false;
        }
        let Some(coalesced) = coalesced_local_edit_record(latest_record, incoming) else {
            return false;
        };
        match coalesced {
            CoalescedEdit::Record(merged_record) => {
                self.replace_coalesced_history_entry(latest_index, &merged_record, now);
                self.latest_operation_record = Some(merged_record);
            }
            CoalescedEdit::Noop => {
                self.history.remove(latest_index);
                self.latest_history_update_at = None;
            }
        }
        true
    }

    fn replace_coalesced_history_entry(
        &mut self,
        latest_index: usize,
        merged_record: &TextDocumentOperationRecord,
        now: Instant,
    ) {
        let source = self.history[latest_index].source;
        let edits = merged_record
            .edits
            .iter()
            .map(|edit| self.history_edit_from_operation_edit(edit, source))
            .collect::<PieceHistoryEdits>();
        let fingerprint = self.fingerprint_for_history_edits(&edits);
        let latest = &mut self.history[latest_index];
        latest.edits = edits;
        latest.next_selection = merged_record.next_selection;
        latest.visible_generation_after = self.piece_tree.generation().min(u32::MAX as u64) as u32;
        latest.fingerprint = fingerprint;
        latest.summary = operation_summary(latest.source, merged_record);
        self.latest_history_update_at = Some(now);
    }

    fn fingerprint_for_history_edits(&self, edits: &[PieceHistoryEdit]) -> u64 {
        let mut parts = Vec::new();
        for edit in edits {
            match edit {
                PieceHistoryEdit::Inserted { span, .. } => {
                    parts.push(self.piece_tree.text_for_span(*span));
                }
                PieceHistoryEdit::Deleted { spans, .. } => {
                    for span in spans {
                        parts.push(self.piece_tree.text_for_span(*span));
                    }
                }
                PieceHistoryEdit::Replaced {
                    deleted, inserted, ..
                } => {
                    for span in deleted {
                        parts.push(self.piece_tree.text_for_span(*span));
                    }
                    parts.push(self.piece_tree.text_for_span(*inserted));
                }
            }
        }
        fingerprint_parts(parts)
    }

    fn enforce_history_budget(&mut self) {
        let mut bytes = self
            .history
            .iter()
            .map(PieceHistoryEntry::byte_cost)
            .sum::<usize>();
        let mut evicted = false;
        while self.history.len() > self.history_budget.per_file_entry_limit
            || bytes as u64 > self.history_budget.per_file_byte_budget
        {
            let removed = self.history.remove(0);
            let cost = removed.byte_cost();
            bytes = bytes.saturating_sub(cost);
            capacity_metrics::record_history_eviction_per_file(cost);
            evicted = true;
        }
        if evicted {
            self.compact_history_storage();
        }
    }

    fn compact_history_storage(&mut self) {
        let mut spans = self.history_spans();
        Arc::make_mut(&mut self.piece_tree).compact_add_buffer(&mut spans);
        self.replace_history_spans(spans);
    }

    fn history_spans(&self) -> Vec<ByteSpan> {
        let mut spans = Vec::new();
        for entry in &self.history {
            for edit in &entry.edits {
                match edit {
                    PieceHistoryEdit::Inserted { span, .. } => spans.push(*span),
                    PieceHistoryEdit::Deleted { spans: deleted, .. } => {
                        spans.extend(deleted.iter().copied());
                    }
                    PieceHistoryEdit::Replaced {
                        deleted, inserted, ..
                    } => {
                        spans.extend(deleted.iter().copied());
                        spans.push(*inserted);
                    }
                }
            }
        }
        spans
    }

    fn replace_history_spans(&mut self, spans: Vec<ByteSpan>) {
        let mut spans = spans.into_iter();
        for entry in &mut self.history {
            for edit in &mut entry.edits {
                match edit {
                    PieceHistoryEdit::Inserted { span, .. } => {
                        if let Some(next) = spans.next() {
                            *span = next;
                        }
                    }
                    PieceHistoryEdit::Deleted { spans: deleted, .. } => {
                        for span in deleted {
                            if let Some(next) = spans.next() {
                                *span = next;
                            }
                        }
                    }
                    PieceHistoryEdit::Replaced {
                        deleted, inserted, ..
                    } => {
                        for span in deleted {
                            if let Some(next) = spans.next() {
                                *span = next;
                            }
                        }
                        if let Some(next) = spans.next() {
                            *inserted = next;
                        }
                    }
                }
            }
        }
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

enum CoalescedEdit {
    Record(TextDocumentOperationRecord),
    Noop,
}

fn coalesced_local_edit_record(
    latest: TextDocumentOperationRecord,
    incoming: &TextDocumentOperationRecord,
) -> Option<CoalescedEdit> {
    if latest.edits.len() != 1 || incoming.edits.len() != 1 {
        return None;
    }
    // Compare by index only — `prefer_next_row` is a UI hint that flips at
    // soft-wrap boundaries, and treating it as a cursor jump would split
    // continuous typing into spurious entries.
    if !cursor_ranges_share_position(&latest.next_selection, &incoming.previous_selection)
        || !latest.next_selection.is_empty()
        || !incoming.next_selection.is_empty()
    {
        return None;
    }
    let latest_edit = latest.edits.first()?;
    let incoming_edit = incoming.edits.first()?;
    if !coalescable_edit_text(latest_edit) || !coalescable_edit_text(incoming_edit) {
        return None;
    }

    if !latest_edit.inserted_text.is_empty() {
        return coalesce_into_inserted_text(latest, incoming);
    }

    if !latest_edit.deleted_text.is_empty() && latest_edit.inserted_text.is_empty() {
        return coalesce_after_delete(latest, incoming);
    }

    None
}

fn cursor_ranges_share_position(left: &CursorRange, right: &CursorRange) -> bool {
    left.primary.index == right.primary.index && left.secondary.index == right.secondary.index
}

fn coalescable_edit_text(edit: &TextDocumentEditOperation) -> bool {
    // Inserts may contain a hard divider (newline) that joins the prior entry
    // and then seals it; only the delete side is restricted to single line.
    is_single_line(&edit.deleted_text)
}

/// Decide whether the latest entry has been "sealed" by a divider in its
/// inserted text. Hard dividers always seal; soft dividers seal only if the
/// user paused after typing them.
fn entry_sealed_by_divider(
    latest: &TextDocumentOperationRecord,
    elapsed: Option<std::time::Duration>,
) -> bool {
    let Some(edit) = latest.edits.last() else {
        return false;
    };
    let Some(last_char) = edit.inserted_text.chars().next_back() else {
        return false;
    };
    if is_hard_divider(last_char) {
        return true;
    }
    if is_soft_divider(last_char) {
        return elapsed.is_none_or(|d| d >= TEXT_HISTORY_SOFT_DIVIDER_PAUSE);
    }
    false
}

fn coalesce_into_inserted_text(
    mut latest: TextDocumentOperationRecord,
    incoming: &TextDocumentOperationRecord,
) -> Option<CoalescedEdit> {
    let latest_edit = latest.edits.first_mut()?;
    let incoming_edit = incoming.edits.first()?;
    let inserted_len = latest_edit.inserted_text.chars().count();
    let deleted_len = incoming_edit.deleted_text.chars().count();
    let inserted_start = latest_edit.start_char;
    let incoming_end = incoming_edit.start_char.checked_add(deleted_len)?;
    let inserted_end = inserted_start.checked_add(inserted_len)?;
    if incoming_edit.start_char < inserted_start || incoming_end > inserted_end {
        return None;
    }

    let relative_start = incoming_edit.start_char - inserted_start;
    let relative_end = relative_start + deleted_len;
    latest_edit.inserted_text = replace_char_range_in_text(
        &latest_edit.inserted_text,
        relative_start..relative_end,
        &incoming_edit.inserted_text,
    );
    latest.next_selection = incoming.next_selection;

    if latest_edit.deleted_text.is_empty() && latest_edit.inserted_text.is_empty() {
        return Some(CoalescedEdit::Noop);
    }

    Some(CoalescedEdit::Record(latest))
}

fn coalesce_after_delete(
    mut latest: TextDocumentOperationRecord,
    incoming: &TextDocumentOperationRecord,
) -> Option<CoalescedEdit> {
    let latest_edit = latest.edits.first_mut()?;
    let incoming_edit = incoming.edits.first()?;
    let latest_start = latest_edit.start_char;
    let incoming_deleted_len = incoming_edit.deleted_text.chars().count();

    if incoming_edit.deleted_text.is_empty() && !incoming_edit.inserted_text.is_empty() {
        if incoming_edit.start_char != latest_start {
            return None;
        }
        latest_edit.inserted_text = incoming_edit.inserted_text.clone();
        latest.next_selection = incoming.next_selection;
        return Some(CoalescedEdit::Record(latest));
    }

    if incoming_edit.inserted_text.is_empty() && !incoming_edit.deleted_text.is_empty() {
        if incoming_edit.start_char == latest_start {
            latest_edit
                .deleted_text
                .push_str(&incoming_edit.deleted_text);
        } else if incoming_edit.start_char + incoming_deleted_len == latest_start {
            latest_edit.start_char = incoming_edit.start_char;
            latest_edit.deleted_text =
                format!("{}{}", incoming_edit.deleted_text, latest_edit.deleted_text);
        } else {
            return None;
        }
        latest_edit.deleted_spans.clear();
        latest.next_selection = incoming.next_selection;
        return Some(CoalescedEdit::Record(latest));
    }

    None
}

fn replace_char_range_in_text(text: &str, range: Range<usize>, replacement: &str) -> String {
    let start = byte_index_for_char(text, range.start);
    let end = byte_index_for_char(text, range.end);
    let mut result = String::with_capacity(text.len() + replacement.len());
    result.push_str(&text[..start]);
    result.push_str(replacement);
    result.push_str(&text[end..]);
    result
}

fn byte_index_for_char(text: &str, char_index: usize) -> usize {
    text.char_indices()
        .map(|(index, _)| index)
        .nth(char_index)
        .unwrap_or(text.len())
}

fn is_single_line(text: &str) -> bool {
    !text.contains('\n') && !text.contains('\r')
}

fn operation_summary(source: PieceSource, operation: &TextDocumentOperationRecord) -> String {
    match source {
        PieceSource::SearchReplace if operation.edits.len() == 1 => "Replace match".to_owned(),
        PieceSource::SearchReplace => format!("Replace {} matches", operation.edits.len()),
        PieceSource::Paste => operation
            .edits
            .first()
            .map(|edit| format!("Paste \"{}\"", super::preview_text(&edit.inserted_text)))
            .unwrap_or_else(|| "Paste".to_owned()),
        PieceSource::Cut => operation
            .edits
            .first()
            .map(|edit| format!("Cut \"{}\"", super::preview_text(&edit.deleted_text)))
            .unwrap_or_else(|| "Cut".to_owned()),
        _ if operation.edits.len() != 1 => format!("Edit {} ranges", operation.edits.len()),
        _ => operation
            .edits
            .first()
            .map(
                |edit| match (edit.deleted_text.is_empty(), edit.inserted_text.is_empty()) {
                    (true, false) => {
                        format!("Insert \"{}\"", super::preview_text(&edit.inserted_text))
                    }
                    (false, true) => {
                        format!("Delete \"{}\"", super::preview_text(&edit.deleted_text))
                    }
                    (false, false) => {
                        format!(
                            "Replace with \"{}\"",
                            super::preview_text(&edit.inserted_text)
                        )
                    }
                    (true, true) => "Edit".to_owned(),
                },
            )
            .unwrap_or_else(|| "Edit".to_owned()),
    }
}

fn persist_cursor_range(range: CursorRange) -> PersistedCursorRange {
    PersistedCursorRange {
        primary_index: range.primary.index,
        primary_prefer_next_row: range.primary.prefer_next_row,
        secondary_index: range.secondary.index,
        secondary_prefer_next_row: range.secondary.prefer_next_row,
    }
}

fn restore_cursor_range(range: PersistedCursorRange) -> CursorRange {
    CursorRange {
        primary: CharCursor {
            index: range.primary_index,
            prefer_next_row: range.primary_prefer_next_row,
        },
        secondary: CharCursor {
            index: range.secondary_index,
            prefer_next_row: range.secondary_prefer_next_row,
        },
    }
}

fn record_expected_parts(
    record: &TextDocumentOperationRecord,
    direction: OperationDirection,
) -> Vec<String> {
    record
        .edits
        .iter()
        .map(|edit| match direction {
            OperationDirection::Undo => edit.inserted_text.clone(),
            OperationDirection::Redo => edit.deleted_text.clone(),
        })
        .collect()
}

fn record_current_parts(
    tree: &PieceTreeLite,
    record: &TextDocumentOperationRecord,
    direction: OperationDirection,
) -> Result<Vec<String>, TextHistoryApplyError> {
    record
        .edits
        .iter()
        .map(|edit| {
            let replaced_len = match direction {
                OperationDirection::Undo => edit.inserted_text.chars().count(),
                OperationDirection::Redo => edit.deleted_text.chars().count(),
            };
            let range = edit.start_char..edit.start_char + replaced_len;
            if range.end > tree.len_chars() {
                return Err(TextHistoryApplyError::OutOfBounds);
            }
            Ok(tree.extract_range(range))
        })
        .collect()
}

fn deleted_spans_or_payload(
    tree: &mut PieceTreeLite,
    edit: &TextDocumentEditOperation,
    source: PieceSource,
) -> Vec<ByteSpan> {
    if !edit.deleted_spans.is_empty() {
        return edit.deleted_spans.clone();
    }
    vec![tree.append_history_text(&edit.deleted_text, source)]
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
    use super::*;
    use crate::app::ui::editor_content::native_editor::EditOperation;
    use std::time::Duration;

    #[test]
    fn adjacent_typing_coalesces_into_one_undo_entry() {
        let mut document = TextDocument::new(String::new());

        insert_edit(&mut document, 0, "a");
        insert_edit(&mut document, 1, "b");
        insert_edit(&mut document, 2, "c");

        assert_eq!(document.extract_text(), "abc");
        assert_eq!(document.operation_undo_depth(), 1);
        assert_eq!(history_record(&document).edits[0].inserted_text, "abc");

        document.undo_last_operation();
        assert_eq!(document.extract_text(), "");
    }

    #[test]
    fn backspace_inside_typing_burst_shrinks_the_insert_entry() {
        let mut document = TextDocument::new(String::new());

        insert_edit(&mut document, 0, "a");
        insert_edit(&mut document, 1, "b");
        insert_edit(&mut document, 2, "c");
        delete_edit(&mut document, 2..3, 3, 2);
        insert_edit(&mut document, 2, "d");

        assert_eq!(document.extract_text(), "abd");
        assert_eq!(document.operation_undo_depth(), 1);
        assert_eq!(history_record(&document).edits[0].inserted_text, "abd");

        document.undo_last_operation();
        assert_eq!(document.extract_text(), "");
    }

    #[test]
    fn mistype_delete_retype_drops_the_transient_mistype() {
        let mut document = TextDocument::new(String::new());

        insert_edit(&mut document, 0, "x");
        delete_edit(&mut document, 0..1, 1, 0);
        insert_edit(&mut document, 0, "y");

        assert_eq!(document.extract_text(), "y");
        assert_eq!(document.operation_undo_depth(), 1);
        assert_eq!(history_record(&document).edits[0].inserted_text, "y");

        document.undo_last_operation();
        assert_eq!(document.extract_text(), "");
    }

    #[test]
    fn removed_transient_edit_does_not_reopen_previous_coalescing_window() {
        let mut document = TextDocument::new(String::new());

        insert_edit(&mut document, 0, "a");
        document.latest_history_update_at = None;
        insert_edit(&mut document, 1, "x");
        delete_edit(&mut document, 1..2, 2, 1);
        insert_edit(&mut document, 1, "y");

        assert_eq!(document.extract_text(), "ay");
        assert_eq!(document.operation_undo_depth(), 2);
        assert_eq!(history_record(&document).edits[0].inserted_text, "a");
        assert_eq!(
            document
                .operation_from_history_entry(&document.history_entries()[1])
                .edits[0]
                .inserted_text,
            "y"
        );
    }

    #[test]
    fn adjacent_backspaces_coalesce_into_one_delete_entry() {
        let mut document = TextDocument::new("abcd".to_owned());

        delete_edit(&mut document, 3..4, 4, 3);
        delete_edit(&mut document, 2..3, 3, 2);
        delete_edit(&mut document, 1..2, 2, 1);

        assert_eq!(document.extract_text(), "a");
        assert_eq!(document.operation_undo_depth(), 1);
        let record = history_record(&document);
        assert_eq!(record.edits[0].start_char, 1);
        assert_eq!(record.edits[0].deleted_text, "bcd");

        document.undo_last_operation();
        assert_eq!(document.extract_text(), "abcd");
    }

    #[test]
    fn long_typing_burst_stays_one_entry() {
        let mut document = TextDocument::new(String::new());
        let phrase = "highlighting should be consistent";

        for (offset, ch) in phrase.chars().enumerate() {
            insert_edit(&mut document, offset, &ch.to_string());
        }

        assert_eq!(document.extract_text(), phrase);
        assert_eq!(document.operation_undo_depth(), 1);
        assert_eq!(history_record(&document).edits[0].inserted_text, phrase);
    }

    #[test]
    fn hard_divider_seals_the_entry() {
        let mut document = TextDocument::new(String::new());

        insert_edit(&mut document, 0, "H");
        insert_edit(&mut document, 1, "i");
        insert_edit(&mut document, 2, ".");
        insert_edit(&mut document, 3, "B");
        insert_edit(&mut document, 4, "y");
        insert_edit(&mut document, 5, "e");

        assert_eq!(document.extract_text(), "Hi.Bye");
        assert_eq!(document.operation_undo_depth(), 2);
        assert_eq!(history_record(&document).edits[0].inserted_text, "Hi.");
        assert_eq!(
            document
                .operation_from_history_entry(&document.history_entries()[1])
                .edits[0]
                .inserted_text,
            "Bye"
        );
    }

    #[test]
    fn newline_seals_the_entry() {
        let mut document = TextDocument::new(String::new());

        insert_edit(&mut document, 0, "a");
        insert_edit(&mut document, 1, "\n");
        insert_edit(&mut document, 2, "b");

        assert_eq!(document.extract_text(), "a\nb");
        assert_eq!(document.operation_undo_depth(), 2);
    }

    #[test]
    fn soft_divider_does_not_seal_inside_a_continuous_burst() {
        let mut document = TextDocument::new(String::new());

        insert_edit(&mut document, 0, "H");
        insert_edit(&mut document, 1, "i");
        insert_edit(&mut document, 2, ",");
        insert_edit(&mut document, 3, " ");
        insert_edit(&mut document, 4, "y");
        insert_edit(&mut document, 5, "o");
        insert_edit(&mut document, 6, "u");

        assert_eq!(document.extract_text(), "Hi, you");
        assert_eq!(document.operation_undo_depth(), 1);
        assert_eq!(history_record(&document).edits[0].inserted_text, "Hi, you");
    }

    #[test]
    fn soft_divider_seals_after_a_pause() {
        let mut document = TextDocument::new(String::new());

        insert_edit(&mut document, 0, "H");
        insert_edit(&mut document, 1, "i");
        insert_edit(&mut document, 2, ",");
        // Simulate the user pausing past the soft-divider seal threshold.
        document.latest_history_update_at =
            Some(Instant::now() - TEXT_HISTORY_SOFT_DIVIDER_PAUSE - Duration::from_millis(50));
        insert_edit(&mut document, 3, " ");

        assert_eq!(document.extract_text(), "Hi, ");
        assert_eq!(document.operation_undo_depth(), 2);
        assert_eq!(history_record(&document).edits[0].inserted_text, "Hi,");
    }

    #[test]
    fn prefer_next_row_flip_does_not_split_a_typing_burst() {
        let mut document = TextDocument::new(String::new());

        document.insert_direct(0, "a");
        document.push_edit_operation(OperationRecord {
            previous_cursor: cursor(0),
            next_cursor: cursor(1),
            edits: vec![EditOperation {
                start_char: 0,
                deleted_text: String::new(),
                inserted_text: "a".to_owned(),
                deleted_spans: Vec::new(),
            }],
        });

        // Same caret position, but the editor reports it with prefer_next_row=true
        // (e.g., the caret sits at the end of a soft-wrapped line).
        document.insert_direct(1, "b");
        let prev_with_flip = CursorRange::one(CharCursor {
            index: 1,
            prefer_next_row: true,
        });
        document.push_edit_operation(OperationRecord {
            previous_cursor: prev_with_flip,
            next_cursor: cursor(2),
            edits: vec![EditOperation {
                start_char: 1,
                deleted_text: String::new(),
                inserted_text: "b".to_owned(),
                deleted_spans: Vec::new(),
            }],
        });

        assert_eq!(document.extract_text(), "ab");
        assert_eq!(document.operation_undo_depth(), 1);
        assert_eq!(history_record(&document).edits[0].inserted_text, "ab");
    }

    #[test]
    fn dash_is_a_soft_divider() {
        let mut document = TextDocument::new(String::new());

        // A continuous burst through the dash stays in one entry.
        for (offset, ch) in "well-known".chars().enumerate() {
            insert_edit(&mut document, offset, &ch.to_string());
        }
        assert_eq!(document.operation_undo_depth(), 1);
        assert_eq!(
            history_record(&document).edits[0].inserted_text,
            "well-known"
        );

        // A burst that ends on a dash, then a pause, seals the entry.
        let mut other = TextDocument::new(String::new());
        for (offset, ch) in "well-".chars().enumerate() {
            insert_edit(&mut other, offset, &ch.to_string());
        }
        other.latest_history_update_at =
            Some(Instant::now() - TEXT_HISTORY_SOFT_DIVIDER_PAUSE - Duration::from_millis(50));
        insert_edit(&mut other, 5, "k");
        assert_eq!(other.operation_undo_depth(), 2);
    }

    #[test]
    fn cursor_jump_starts_a_new_undo_entry() {
        let mut document = TextDocument::new(String::new());

        insert_edit(&mut document, 0, "a");
        insert_edit_with_cursor(&mut document, 0, "b", 0, 1);

        assert_eq!(document.extract_text(), "ba");
        assert_eq!(document.operation_undo_depth(), 2);
    }

    #[test]
    fn keyboard_redo_replays_one_history_entry_at_a_time() {
        let mut document = TextDocument::new(String::new());
        insert_isolated_edit(&mut document, 0, "a");
        insert_isolated_edit(&mut document, 1, "b");
        insert_isolated_edit(&mut document, 2, "c");

        document.undo_last_operation();
        document.undo_last_operation();

        assert_eq!(document.extract_text(), "a");
        assert_eq!(document.operation_redo_depth(), 2);

        document.redo_last_operation();

        assert_eq!(document.extract_text(), "ab");
        assert_eq!(document.operation_undo_depth(), 2);
        assert_eq!(document.operation_redo_depth(), 1);

        document.redo_last_operation();

        assert_eq!(document.extract_text(), "abc");
        assert_eq!(document.operation_undo_depth(), 3);
        assert_eq!(document.operation_redo_depth(), 0);
    }

    #[test]
    fn target_history_redo_replays_through_the_clicked_entry() {
        let mut document = TextDocument::new(String::new());
        insert_isolated_edit(&mut document, 0, "a");
        insert_isolated_edit(&mut document, 1, "b");
        insert_isolated_edit(&mut document, 2, "c");
        let clicked_entry = document.history_entries()[2].id;

        document.undo_last_operation();
        document.undo_last_operation();

        document.apply_text_history_redo(clicked_entry).unwrap();

        assert_eq!(document.extract_text(), "abc");
        assert_eq!(document.operation_undo_depth(), 3);
        assert_eq!(document.operation_redo_depth(), 0);
    }

    #[test]
    fn target_history_undo_replays_clicked_entry_and_later_entries() {
        let mut document = TextDocument::new(String::new());
        insert_isolated_edit(&mut document, 0, "a");
        insert_isolated_edit(&mut document, 1, "b");
        insert_isolated_edit(&mut document, 2, "c");
        let clicked_entry = document.history_entries()[1].id;

        document.apply_text_history_undo(clicked_entry).unwrap();

        assert_eq!(document.extract_text(), "a");
        assert_eq!(document.operation_undo_depth(), 1);
        assert_eq!(document.operation_redo_depth(), 2);
    }

    fn insert_edit(document: &mut TextDocument, start: usize, text: &str) {
        insert_edit_with_cursor(document, start, text, start, start + text.chars().count());
    }

    fn insert_isolated_edit(document: &mut TextDocument, start: usize, text: &str) {
        document.latest_history_update_at = None;
        insert_edit(document, start, text);
    }

    fn insert_edit_with_cursor(
        document: &mut TextDocument,
        start: usize,
        text: &str,
        previous_cursor: usize,
        next_cursor: usize,
    ) {
        document.insert_direct(start, text);
        document.push_edit_operation(OperationRecord {
            previous_cursor: cursor(previous_cursor),
            next_cursor: cursor(next_cursor),
            edits: vec![EditOperation {
                start_char: start,
                deleted_text: String::new(),
                inserted_text: text.to_owned(),
                deleted_spans: Vec::new(),
            }],
        });
    }

    fn delete_edit(
        document: &mut TextDocument,
        range: Range<usize>,
        previous_cursor: usize,
        next_cursor: usize,
    ) {
        let deleted_text = document.piece_tree().extract_range(range.clone());
        let deleted_spans = document.byte_spans_for_range(range.clone());
        document.delete_char_range_direct(range.clone());
        document.push_edit_operation(OperationRecord {
            previous_cursor: cursor(previous_cursor),
            next_cursor: cursor(next_cursor),
            edits: vec![EditOperation {
                start_char: range.start,
                deleted_text,
                inserted_text: String::new(),
                deleted_spans,
            }],
        });
    }

    fn history_record(document: &TextDocument) -> TextDocumentOperationRecord {
        document.operation_from_history_entry(&document.history_entries()[0])
    }

    fn cursor(index: usize) -> CursorRange {
        CursorRange::one(CharCursor::new(index))
    }
}
