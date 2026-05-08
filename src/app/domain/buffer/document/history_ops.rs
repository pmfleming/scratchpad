use super::super::history::{
    CoalescedEdit, OperationDirection, TextDocumentEditOperation, TextHistoryApplyError,
    coalesced_local_edit_record, deleted_spans_or_payload, entry_sealed_by_divider,
    operation_summary, persist_cursor_range, record_current_parts, record_expected_parts,
    restore_cursor_range,
};
use super::super::{
    ByteSpan, PersistedHistoryEdit, PersistedHistoryEntry, PieceHistoryEdit, PieceHistoryEdits,
    PieceHistoryEntry, PieceHistoryFlags, PieceSource, TEXT_HISTORY_COALESCE_WINDOW,
    TextDocumentOperationRecord, fingerprint_parts, next_text_history_global_seq,
    register_text_history_global_seq,
};
use super::TextDocument;
use crate::app::capacity_metrics;
use crate::app::ui::editor_content::native_editor::CursorRange;
use std::sync::Arc;
use std::time::Instant;

impl TextDocument {
    pub(super) fn replay_last_operation(
        &mut self,
        direction: OperationDirection,
    ) -> Option<CursorRange> {
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

    pub(super) fn apply_text_history_entry(
        &mut self,
        entry_id: u64,
        direction: OperationDirection,
    ) -> Result<CursorRange, TextHistoryApplyError> {
        let index = self
            .history
            .iter()
            .position(|entry| entry.id == entry_id)
            .ok_or(TextHistoryApplyError::OutOfBounds)?;
        let indices = self.replayable_indices_at(index, direction)?;

        let mut applied_selection = None;
        for idx in indices {
            if !self.history[idx].flags.replayable {
                return Err(TextHistoryApplyError::Conflict);
            }
            let record = self.operation_from_history_entry(&self.history[idx]);
            self.validate_text_history_record(idx, &record, direction)?;
            self.apply_operation_record(&record, direction);
            self.history[idx].flags.undone = matches!(direction, OperationDirection::Undo);
            self.latest_operation_record = Some(record.clone());
            applied_selection = Some(direction.selection(&record));
        }
        if applied_selection.is_some() {
            self.latest_history_update_at = None;
            self.pending_history_generation_before = None;
        }
        self.revision_counter = self.revision_counter.wrapping_add(1);
        applied_selection.ok_or(TextHistoryApplyError::Conflict)
    }

    pub(super) fn operation_from_history_entry(
        &self,
        entry: &PieceHistoryEntry,
    ) -> TextDocumentOperationRecord {
        let edits = entry
            .edits
            .iter()
            .map(|edit| TextDocumentEditOperation {
                start_char: edit.start_char() as usize,
                deleted_text: self.text_for_spans(edit.deleted_spans()),
                inserted_text: edit
                    .inserted_span()
                    .map(|span| self.piece_tree.text_for_span(span).to_owned())
                    .unwrap_or_default(),
                deleted_spans: edit.deleted_spans().to_vec(),
            })
            .collect();
        TextDocumentOperationRecord {
            previous_selection: entry.previous_selection,
            next_selection: entry.next_selection,
            edits,
        }
    }

    pub(super) fn push_operation_record(
        &mut self,
        mut record: TextDocumentOperationRecord,
        source: PieceSource,
    ) {
        record
            .edits
            .retain(|edit| !edit.deleted_text.is_empty() || !edit.inserted_text.is_empty());
        if record.edits.is_empty() {
            self.pending_history_generation_before = None;
            return;
        }

        self.latest_operation_record = Some(record.clone());
        let old_len = self.history.len();
        self.history.retain(|entry| !entry.is_undone());
        if self.history.len() != old_len {
            self.latest_history_update_at = None;
        }
        if self.try_coalesce_history(&record, source) {
            self.revision_counter = self.revision_counter.wrapping_add(1);
            self.pending_history_generation_before = None;
            return;
        }

        let entry = self.history_entry_from_operation(record, source);
        self.history.push(entry);
        self.revision_counter = self.revision_counter.wrapping_add(1);
        self.enforce_history_budget();
        self.pending_history_generation_before = None;
    }

    fn apply_operation_record(
        &mut self,
        record: &TextDocumentOperationRecord,
        direction: OperationDirection,
    ) {
        let ordered: Box<dyn Iterator<Item = &TextDocumentEditOperation>> = match direction {
            OperationDirection::Undo => Box::new(record.edits.iter().rev()),
            OperationDirection::Redo => Box::new(record.edits.iter()),
        };
        for edit in ordered {
            let replaced_len = edit.expected_text(direction).chars().count();
            self.replace_char_range_raw(
                edit.start_char..edit.start_char + replaced_len,
                edit.replacement_text(direction),
            );
        }
    }

    fn replayable_indices_at(
        &self,
        index: usize,
        direction: OperationDirection,
    ) -> Result<Vec<usize>, TextHistoryApplyError> {
        let entry_undone = self.history[index].is_undone();
        match direction {
            OperationDirection::Undo if entry_undone => Err(TextHistoryApplyError::Conflict),
            OperationDirection::Redo if !entry_undone => Err(TextHistoryApplyError::Conflict),
            OperationDirection::Undo => Ok((index..self.history.len())
                .rev()
                .filter(|i| !self.history[*i].is_undone())
                .collect()),
            OperationDirection::Redo => Ok((0..=index)
                .filter(|i| self.history[*i].is_undone())
                .collect()),
        }
    }

    fn validate_text_history_record(
        &self,
        index: usize,
        record: &TextDocumentOperationRecord,
        direction: OperationDirection,
    ) -> Result<(), TextHistoryApplyError> {
        let expected_generation = self.history.get(index).map(|entry| match direction {
            OperationDirection::Undo => entry.visible_generation_after,
            OperationDirection::Redo => entry.visible_generation_before,
        });
        if expected_generation == Some(self.visible_generation()) {
            return Ok(());
        }

        let expected_parts = record_expected_parts(record, direction);
        let expected_fingerprint = fingerprint_parts(expected_parts.iter().copied());
        let current_fingerprint = fingerprint_parts(
            record_current_parts(self.piece_tree.as_ref(), record, direction)?
                .iter()
                .map(String::as_str),
        );
        if expected_fingerprint == current_fingerprint {
            return Ok(());
        }

        record.edits.iter().try_for_each(|edit| {
            let expected = edit.expected_text(direction);
            let range = edit.start_char..edit.start_char + expected.chars().count();
            if range.end > self.piece_tree.len_chars() {
                return Err(TextHistoryApplyError::OutOfBounds);
            }
            if !expected.is_empty() && self.piece_tree.extract_range(range) != expected {
                return Err(TextHistoryApplyError::Conflict);
            }
            Ok(())
        })
    }

    fn text_for_spans(&self, spans: &[ByteSpan]) -> String {
        let mut text = String::new();
        for span in spans {
            text.push_str(self.piece_tree.text_for_span(*span));
        }
        text
    }

    pub(super) fn export_history_entry(&self, entry: &PieceHistoryEntry) -> PersistedHistoryEntry {
        PersistedHistoryEntry {
            id: entry.id,
            global_seq: entry.global_seq,
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

    pub(super) fn import_history_entry(
        &mut self,
        persisted: PersistedHistoryEntry,
    ) -> PieceHistoryEntry {
        let all_payloads = persisted.has_all_payloads();
        let edits = persisted
            .edits
            .into_iter()
            .map(|edit| self.import_history_edit(edit, persisted.source))
            .collect::<PieceHistoryEdits>();
        let restored_fingerprint = self.fingerprint_for_history_edits(&edits);
        let mut flags = persisted.flags;
        flags.replayable &= all_payloads && restored_fingerprint == persisted.fingerprint;
        register_text_history_global_seq(persisted.global_seq);
        PieceHistoryEntry {
            id: persisted.id,
            global_seq: persisted.global_seq,
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

    pub(super) fn enforce_history_budget(&mut self) {
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
            self.revision_counter = self.revision_counter.wrapping_add(1);
            self.compact_history_storage();
        }
    }

    pub(super) fn compact_history_storage(&mut self) {
        let mut spans = self.history_spans();
        Arc::make_mut(&mut self.piece_tree).compact_add_buffer(&mut spans);
        self.replace_history_spans(spans);
    }

    fn export_history_edit(&self, edit: &PieceHistoryEdit) -> PersistedHistoryEdit {
        edit.to_persisted(|span| self.piece_tree.text_for_span(span).to_owned())
    }

    fn import_history_edit(
        &mut self,
        edit: PersistedHistoryEdit,
        source: PieceSource,
    ) -> PieceHistoryEdit {
        let tree = Arc::make_mut(&mut self.piece_tree);
        edit.into_piece(|text| tree.append_history_text(text, source))
    }

    fn history_entry_from_operation(
        &mut self,
        record: TextDocumentOperationRecord,
        source: PieceSource,
    ) -> PieceHistoryEntry {
        let generation_after = self.visible_generation();
        debug_assert!(
            self.pending_history_generation_before.is_some(),
            "history generation_before should be captured before pushing text history"
        );
        let generation_before = self
            .pending_history_generation_before
            .unwrap_or(generation_after);
        let edits = record
            .edits
            .iter()
            .map(|edit| self.history_edit_from_operation_edit(edit, source))
            .collect::<PieceHistoryEdits>();
        let fingerprint = self.fingerprint_for_history_edits(&edits);
        self.latest_history_update_at = Some(Instant::now());
        let entry = PieceHistoryEntry {
            id: self.next_history_id,
            global_seq: next_text_history_global_seq(),
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
            (true, true) => {
                debug_assert!(
                    false,
                    "no-op text records should be filtered before history entry creation"
                );
                PieceHistoryEdit::Inserted {
                    start_char,
                    span: ByteSpan {
                        buffer: super::super::piece_tree::PieceBuffer::Add,
                        start_byte: 0,
                        byte_len: 0,
                    },
                }
            }
        }
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
        let visible_generation_after = self.visible_generation();
        let latest = &mut self.history[latest_index];
        latest.edits = edits;
        latest.next_selection = merged_record.next_selection;
        latest.visible_generation_after = visible_generation_after;
        latest.global_seq = next_text_history_global_seq();
        latest.fingerprint = fingerprint;
        latest.summary = operation_summary(latest.source, merged_record);
        self.latest_history_update_at = Some(now);
    }

    fn history_spans(&self) -> Vec<ByteSpan> {
        self.history
            .iter()
            .flat_map(|entry| entry.edits.iter())
            .flat_map(PieceHistoryEdit::spans)
            .collect()
    }

    fn replace_history_spans(&mut self, spans: Vec<ByteSpan>) {
        let expected = self.history_spans().len();
        debug_assert_eq!(
            expected,
            spans.len(),
            "history span replacement count mismatch"
        );
        let mut spans = spans.into_iter();
        for entry in &mut self.history {
            for edit in &mut entry.edits {
                edit.each_span_mut(|slot| {
                    if let Some(next) = spans.next() {
                        *slot = next;
                    } else {
                        debug_assert!(false, "history span replacement iterator ran short");
                    }
                });
            }
        }
        debug_assert!(
            spans.next().is_none(),
            "history span replacement iterator had extra spans"
        );
    }

    pub(super) fn normalize_imported_redo_state(&mut self) {
        let mut in_undone_suffix = true;
        for entry in self.history.iter_mut().rev() {
            if in_undone_suffix && entry.is_undone() {
                continue;
            }
            in_undone_suffix = false;
            if entry.is_undone() {
                // The runtime model is linear: redoable entries are exactly the
                // newest contiguous undone suffix. Older imported undone flags
                // cannot be trusted, so keep the row as historical but disable
                // replay rather than allowing redo to skip newer applied edits.
                entry.flags.undone = false;
                entry.flags.replayable = false;
            }
        }
    }
}
