use super::super::super::history::{persist_cursor_range, restore_cursor_range};
use super::super::super::{
    PersistedHistoryEdit, PersistedHistoryEntry, PieceHistoryEdit, PieceHistoryEdits,
    PieceHistoryEntry, PieceSource, register_text_history_global_seq,
};
use super::super::TextDocument;
use std::sync::Arc;

impl TextDocument {
    pub(in crate::app::domain::buffer::document) fn export_history_entry(
        &self,
        entry: &PieceHistoryEntry,
    ) -> PersistedHistoryEntry {
        PersistedHistoryEntry {
            id: entry.id,
            parent_id: entry.parent_id,
            edited_at: entry.edited_at,
            current: self.history.tree.now() == entry.id,
            selected_tip: self.history.tree.tip() == entry.id,
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
            parent_id: persisted.parent_id,
            edited_at: persisted.edited_at,
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

    fn export_history_edit(&self, edit: &PieceHistoryEdit) -> PersistedHistoryEdit {
        edit.to_persisted(|span| self.content.piece_tree.text_for_span(span).to_string())
    }

    fn import_history_edit(
        &mut self,
        edit: PersistedHistoryEdit,
        source: PieceSource,
    ) -> PieceHistoryEdit {
        let tree = Arc::make_mut(&mut self.content.piece_tree);
        edit.into_piece(|text| tree.append_history_text(text, source))
    }
}
