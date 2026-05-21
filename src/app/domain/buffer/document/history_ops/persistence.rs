use super::super::super::history::{persist_cursor_range, restore_cursor_range};
use super::super::super::{
    ByteSpan, PersistedHistoryEdit, PersistedHistoryEntry, PieceHistoryEdit, PieceHistoryEdits,
    PieceHistoryEntry, PieceSource, register_text_history_global_seq,
};
use super::super::TextDocument;
use crate::app::capacity_metrics;
use std::sync::Arc;

impl TextDocument {
    pub(in crate::app::domain::buffer::document) fn export_history_entry(
        &self,
        entry: &PieceHistoryEntry,
    ) -> PersistedHistoryEntry {
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

    pub(in crate::app::domain::buffer::document) fn import_history_entry(
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

    pub(in crate::app::domain::buffer::document) fn enforce_history_budget(&mut self) {
        let mut remove_count = self
            .history
            .entries
            .len()
            .saturating_sub(self.history.budget.per_file_entry_limit);
        let mut retained_bytes = self.history.byte_usage;
        for (index, entry) in self.history.entries.iter().enumerate() {
            if index < remove_count {
                retained_bytes = retained_bytes.saturating_sub(entry.byte_cost());
                continue;
            }
            if retained_bytes as u64 <= self.history.budget.per_file_byte_budget {
                break;
            }
            retained_bytes = retained_bytes.saturating_sub(entry.byte_cost());
            remove_count = index + 1;
        }

        if remove_count > 0 {
            let removed_entries = self
                .history
                .entries
                .drain(0..remove_count)
                .collect::<Vec<_>>();
            for removed in removed_entries {
                let cost = removed.byte_cost();
                self.history.byte_usage = self.history.byte_usage.saturating_sub(cost);
                self.remove_history_depth(&removed);
                capacity_metrics::record_history_eviction_per_file(cost);
            }
            self.history.revision_counter = self.history.revision_counter.wrapping_add(1);
            self.compact_history_storage();
        }
    }

    pub(in crate::app::domain::buffer::document) fn compact_history_storage(&mut self) {
        let mut spans = self.history_spans();
        Arc::make_mut(&mut self.content.piece_tree).compact_add_buffer(&mut spans);
        self.replace_history_spans(spans);
    }

    pub(in crate::app::domain::buffer::document) fn normalize_imported_redo_state(&mut self) {
        let mut in_undone_suffix = true;
        for entry in self.history.entries.iter_mut().rev() {
            if in_undone_suffix && entry.is_undone() {
                continue;
            }
            in_undone_suffix = false;
            if entry.is_undone() {
                entry.flags.undone = false;
                entry.flags.replayable = false;
            }
        }
    }

    fn export_history_edit(&self, edit: &PieceHistoryEdit) -> PersistedHistoryEdit {
        edit.to_persisted(|span| self.content.piece_tree.text_for_span(span).to_owned())
    }

    fn import_history_edit(
        &mut self,
        edit: PersistedHistoryEdit,
        source: PieceSource,
    ) -> PieceHistoryEdit {
        let tree = Arc::make_mut(&mut self.content.piece_tree);
        edit.into_piece(|text| tree.append_history_text(text, source))
    }

    fn history_spans(&self) -> Vec<ByteSpan> {
        self.history
            .entries
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
        for entry in &mut self.history.entries {
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
}
