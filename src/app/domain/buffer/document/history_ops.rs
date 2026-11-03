mod persistence;
pub(crate) use persistence::HistorySnapshot;

use super::super::history::{
    CoalescedEdit, OperationDirection, TextDocumentEditOperation, coalesced_local_edit_record,
    deleted_spans_or_payload, entry_sealed_by_divider, operation_summary,
};
use super::super::{
    ByteSpan, PieceHistoryEdit, PieceHistoryEdits, PieceHistoryEntry, PieceHistoryFlags,
    PieceSource, TEXT_HISTORY_COALESCE_WINDOW, TextDocumentOperationRecord,
    next_text_history_global_seq,
};
use super::TextDocument;
use crate::app::domain::CursorRange;
use std::sync::Arc;
use std::time::Instant;

impl TextDocument {
    pub(super) fn replay_last_operation(
        &mut self,
        direction: OperationDirection,
    ) -> Option<CursorRange> {
        let target = match direction {
            OperationDirection::Undo => self.history.tree.parent(self.history.tree.now())?,
            OperationDirection::Redo => self.history.tree.redo_target()?,
        };
        self.navigate_history(target).ok()
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
                    .map(|span| self.content.piece_tree.text_for_span(span).to_string())
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
        if self.try_coalesce_history(&record, source) {
            self.advance_history_revision();
            self.history.pending_generation_before = None;
            return;
        }

        let entry = self.history_entry_from_operation(record, source);
        self.history.byte_usage += entry.byte_cost();
        assert!(self.history.tree.append(entry.id));
        self.history.entries.push(entry);
        self.advance_history_revision();

        self.history.pending_generation_before = None;
    }

    pub(super) fn text_for_spans(&self, spans: &[ByteSpan]) -> String {
        let mut text = String::new();
        for span in spans {
            text.push_str(&self.content.piece_tree.text_for_span(*span));
        }
        text
    }

    pub(super) fn history_entry_from_operation(
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
            parent_id: self.history.tree.now(),
            edited_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
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
            || latest.id != self.history.tree.now()
            || latest.id != self.history.tree.tip()
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
                let Some(removed) = self.history.entries.pop() else {
                    return false;
                };
                self.history.byte_usage =
                    self.history.byte_usage.saturating_sub(removed.byte_cost());
                assert!(self.history.tree.cancel_open_leaf(removed.id));
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
        latest.edited_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
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
