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
        if self.piece_tree().read_fault().is_some() {
            return;
        }
        record
            .edits
            .retain(|edit| !edit.deleted_text.is_empty() || !edit.inserted_text.is_empty());
        if record.edits.is_empty() {
            return;
        }

        if self.try_coalesce_history(&record, source) {
            self.advance_history_revision();

            return;
        }

        self.history.latest_operation_record = bounded_record(record.clone());
        let entry = self.history_entry_from_operation(record, source);
        self.history.byte_usage += entry.byte_cost();
        assert!(self.history.tree.append(entry.id));
        self.history.entries.push(entry);
        self.advance_history_revision();
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
        let edits = record
            .edits
            .iter()
            .enumerate()
            .map(|(index, edit)| {
                let span = self.inserted_reference(&record, index);
                self.history_edit_from_operation_edit(edit, source, span)
            })
            .collect::<PieceHistoryEdits>();
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

            summary: operation_summary(source, &record),
            edits,
            flags: PieceHistoryFlags {
                undone: false,
                replayable: true,
            },
            previous_selection: record.previous_selection,
            next_selection: record.next_selection,
        };
        self.history.next_id = self.history.next_id.saturating_add(1);
        entry
    }

    fn inserted_reference(
        &self,
        record: &TextDocumentOperationRecord,
        index: usize,
    ) -> Option<ByteSpan> {
        let edit = &record.edits[index];
        if edit.inserted_text.is_empty() {
            return None;
        }
        let mut start = edit.start_char;
        for later in &record.edits[index + 1..] {
            if later.start_char <= start {
                start = start
                    .checked_sub(later.deleted_text.chars().count())?
                    .checked_add(later.inserted_text.chars().count())?;
            }
        }
        let spans = self
            .byte_spans_for_range(start..start.checked_add(edit.inserted_text.chars().count())?);
        let mut merged = *spans.first()?;
        for span in &spans[1..] {
            if span.buffer != merged.buffer || span.start_byte != merged.byte_end() {
                return None;
            }
            merged.byte_len += span.byte_len;
            merged.char_len += span.char_len;
        }
        (merged.byte_len == edit.inserted_text.len() as u64).then_some(merged)
    }

    fn history_edit_from_operation_edit(
        &mut self,
        edit: &TextDocumentEditOperation,
        source: PieceSource,
        inserted: Option<ByteSpan>,
    ) -> PieceHistoryEdit {
        let tree = Arc::make_mut(&mut self.content.piece_tree);
        let start_char = edit.start_char as u64;
        match (edit.deleted_text.is_empty(), edit.inserted_text.is_empty()) {
            (true, false) => PieceHistoryEdit::Inserted {
                start_char,
                span: inserted
                    .unwrap_or_else(|| tree.append_history_text(&edit.inserted_text, source)),
            },
            (false, true) => PieceHistoryEdit::Deleted {
                start_char,
                spans: deleted_spans_or_payload(tree, edit, source),
            },
            (false, false) => PieceHistoryEdit::Replaced {
                start_char,
                deleted: deleted_spans_or_payload(tree, edit, source),
                inserted: inserted
                    .unwrap_or_else(|| tree.append_history_text(&edit.inserted_text, source)),
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
                        char_len: 0,
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
        // This record is retained while the group is open. Re-reading the old
        // history spans here could fail after the incoming edit was applied.
        let Some(latest_record) = self.history.latest_operation_record.clone() else {
            return false;
        };
        if entry_sealed_by_divider(&latest_record, elapsed) {
            return false;
        }
        let Some(coalesced) = coalesced_local_edit_record(latest_record, incoming) else {
            return false;
        };
        match coalesced {
            CoalescedEdit::Record(merged_record) => {
                self.replace_coalesced_history_entry(latest_index, &merged_record, now);
                self.history.latest_operation_record = bounded_record(merged_record);
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
            .enumerate()
            .map(|(index, edit)| {
                let span = self.inserted_reference(merged_record, index);
                self.history_edit_from_operation_edit(edit, source, span)
            })
            .collect::<PieceHistoryEdits>();

        let old_cost = self.history.entries[latest_index].byte_cost();
        let latest = &mut self.history.entries[latest_index];
        latest.edits = edits;
        latest.next_selection = merged_record.next_selection;

        latest.global_seq = next_text_history_global_seq();
        latest.edited_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
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

pub(super) fn bounded_record(
    record: TextDocumentOperationRecord,
) -> Option<TextDocumentOperationRecord> {
    let bytes = record
        .edits
        .iter()
        .map(|e| e.deleted_text.len().saturating_add(e.inserted_text.len()))
        .fold(0usize, usize::saturating_add);
    (bytes <= super::super::state::BULK_TEXT_BYTES).then_some(record)
}
