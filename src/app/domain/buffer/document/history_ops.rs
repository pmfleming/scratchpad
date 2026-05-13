mod persistence;

use super::super::history::{
    CoalescedEdit, OperationDirection, TextDocumentEditOperation, TextHistoryApplyError,
    coalesced_local_edit_record, deleted_spans_or_payload, entry_sealed_by_divider,
    operation_summary, record_current_parts, record_expected_parts,
};
use super::super::{
    ByteSpan, PieceHistoryEdit, PieceHistoryEdits, PieceHistoryEntry, PieceHistoryFlags,
    PieceSource, TEXT_HISTORY_COALESCE_WINDOW, TextDocumentOperationRecord, fingerprint_parts,
    next_text_history_global_seq,
};
use super::TextDocument;
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
                .entries
                .iter()
                .rev()
                .find(|entry| !entry.is_undone() && entry.flags.replayable)
                .map(|entry| entry.id)?,
            OperationDirection::Redo => self
                .history
                .entries
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
            .entries
            .iter()
            .position(|entry| entry.id == entry_id)
            .ok_or(TextHistoryApplyError::OutOfBounds)?;
        let indices = self.replayable_indices_at(index, direction)?;

        let mut candidate = self.clone();
        let selection = candidate.apply_text_history_indices(indices, direction)?;
        *self = candidate;
        Ok(selection)
    }

    fn apply_text_history_indices(
        &mut self,
        indices: Vec<usize>,
        direction: OperationDirection,
    ) -> Result<CursorRange, TextHistoryApplyError> {
        let mut applied_selection = None;
        for idx in indices {
            if !self.history.entries[idx].flags.replayable {
                return Err(TextHistoryApplyError::Conflict);
            }
            let record = self.operation_from_history_entry(&self.history.entries[idx]);
            self.validate_text_history_record(idx, &record, direction)?;
            self.apply_operation_record(&record, direction);
            self.mark_history_entry_undone(idx, matches!(direction, OperationDirection::Undo));
            self.history.latest_operation_record = Some(record.clone());
            applied_selection = Some(direction.selection(&record));
        }
        if applied_selection.is_some() {
            self.history.latest_update_at = None;
            self.history.pending_generation_before = None;
        }
        self.history.revision_counter = self.history.revision_counter.wrapping_add(1);
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
                    .map(|span| self.content.piece_tree.text_for_span(span).to_owned())
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
            self.history.pending_generation_before = None;
            return;
        }

        self.history.latest_operation_record = Some(record.clone());
        let old_len = self.history.entries.len();
        let mut removed_bytes = 0usize;
        let mut removed_redo_depth = 0usize;
        self.history.entries.retain(|entry| {
            let keep = !entry.is_undone();
            if !keep {
                removed_bytes += entry.byte_cost();
                removed_redo_depth += 1;
            }
            keep
        });
        if self.history.entries.len() != old_len {
            self.history.byte_usage = self.history.byte_usage.saturating_sub(removed_bytes);
            self.history.redo_depth = self.history.redo_depth.saturating_sub(removed_redo_depth);
            self.history.latest_update_at = None;
        }
        if self.try_coalesce_history(&record, source) {
            self.history.revision_counter = self.history.revision_counter.wrapping_add(1);
            self.history.pending_generation_before = None;
            return;
        }

        let entry = self.history_entry_from_operation(record, source);
        self.history.byte_usage += entry.byte_cost();
        self.add_history_depth(&entry);
        self.history.entries.push(entry);
        self.history.revision_counter = self.history.revision_counter.wrapping_add(1);
        self.enforce_history_budget();
        self.history.pending_generation_before = None;
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
        let entry_undone = self.history.entries[index].is_undone();
        match direction {
            OperationDirection::Undo if entry_undone => Err(TextHistoryApplyError::Conflict),
            OperationDirection::Redo if !entry_undone => Err(TextHistoryApplyError::Conflict),
            OperationDirection::Undo => Ok((index..self.history.entries.len())
                .rev()
                .filter(|i| !self.history.entries[*i].is_undone())
                .collect()),
            OperationDirection::Redo => Ok((0..=index)
                .filter(|i| self.history.entries[*i].is_undone())
                .collect()),
        }
    }

    fn validate_text_history_record(
        &self,
        index: usize,
        record: &TextDocumentOperationRecord,
        direction: OperationDirection,
    ) -> Result<(), TextHistoryApplyError> {
        let expected_generation = self
            .history
            .entries
            .get(index)
            .map(|entry| match direction {
                OperationDirection::Undo => entry.visible_generation_after,
                OperationDirection::Redo => entry.visible_generation_before,
            });
        if expected_generation == Some(self.visible_generation()) {
            return Ok(());
        }

        let expected_parts = record_expected_parts(record, direction);
        let expected_fingerprint = fingerprint_parts(expected_parts.iter().copied());
        let current_fingerprint = fingerprint_parts(
            record_current_parts(self.content.piece_tree.as_ref(), record, direction)?
                .iter()
                .map(String::as_str),
        );
        if expected_fingerprint == current_fingerprint {
            return Ok(());
        }

        record.edits.iter().try_for_each(|edit| {
            let expected = edit.expected_text(direction);
            let range = edit.start_char..edit.start_char + expected.chars().count();
            if range.end > self.content.piece_tree.len_chars() {
                return Err(TextHistoryApplyError::OutOfBounds);
            }
            if !expected.is_empty() && self.content.piece_tree.extract_range(range) != expected {
                return Err(TextHistoryApplyError::Conflict);
            }
            Ok(())
        })
    }

    fn text_for_spans(&self, spans: &[ByteSpan]) -> String {
        let mut text = String::new();
        for span in spans {
            text.push_str(self.content.piece_tree.text_for_span(*span));
        }
        text
    }

    fn history_entry_from_operation(
        &mut self,
        record: TextDocumentOperationRecord,
        source: PieceSource,
    ) -> PieceHistoryEntry {
        let generation_after = self.visible_generation();
        debug_assert!(
            self.history.pending_generation_before.is_some(),
            "history generation_before should be captured before pushing text history"
        );
        let generation_before = self
            .history
            .pending_generation_before
            .unwrap_or(generation_after);
        let edits = record
            .edits
            .iter()
            .map(|edit| self.history_edit_from_operation_edit(edit, source))
            .collect::<PieceHistoryEdits>();
        let fingerprint = self.fingerprint_for_history_edits(&edits);
        self.history.latest_update_at = Some(Instant::now());
        let entry = PieceHistoryEntry {
            id: self.history.next_id,
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
        self.history.next_id = self.history.next_id.saturating_add(1);
        entry
    }

    fn history_edit_from_operation_edit(
        &mut self,
        edit: &TextDocumentEditOperation,
        source: PieceSource,
    ) -> PieceHistoryEdit {
        let tree = Arc::make_mut(&mut self.content.piece_tree);
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
        let Some(latest_index) = self.history.entries.len().checked_sub(1) else {
            return false;
        };
        let latest = &self.history.entries[latest_index];
        let now = Instant::now();
        let elapsed = self
            .history
            .latest_update_at
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
                self.history.latest_operation_record = Some(merged_record);
            }
            CoalescedEdit::Noop => {
                let removed = self.history.entries.remove(latest_index);
                self.history.byte_usage =
                    self.history.byte_usage.saturating_sub(removed.byte_cost());
                self.remove_history_depth(&removed);
                self.history.latest_update_at = None;
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
        let source = self.history.entries[latest_index].source;
        let edits = merged_record
            .edits
            .iter()
            .map(|edit| self.history_edit_from_operation_edit(edit, source))
            .collect::<PieceHistoryEdits>();
        let fingerprint = self.fingerprint_for_history_edits(&edits);
        let visible_generation_after = self.visible_generation();
        let old_cost = self.history.entries[latest_index].byte_cost();
        let latest = &mut self.history.entries[latest_index];
        latest.edits = edits;
        latest.next_selection = merged_record.next_selection;
        latest.visible_generation_after = visible_generation_after;
        latest.global_seq = next_text_history_global_seq();
        latest.fingerprint = fingerprint;
        latest.summary = operation_summary(latest.source, merged_record);
        let new_cost = latest.byte_cost();
        self.history.byte_usage = self
            .history
            .byte_usage
            .saturating_sub(old_cost)
            .saturating_add(new_cost);
        self.history.latest_update_at = Some(now);
    }
}
