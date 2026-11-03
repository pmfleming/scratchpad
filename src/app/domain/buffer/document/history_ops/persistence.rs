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
        export_entry(
            entry,
            &self.content.piece_tree,
            self.history_now(),
            self.history_tip(),
        )
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

    fn import_history_edit(
        &mut self,
        edit: PersistedHistoryEdit,
        source: PieceSource,
    ) -> PieceHistoryEdit {
        let tree = Arc::make_mut(&mut self.content.piece_tree);
        edit.into_piece(|text| tree.append_history_text(text, source))
    }
}

fn export_entry(
    entry: &PieceHistoryEntry,
    tree: &super::super::super::PieceTreeLite,
    now: u64,
    tip: u64,
) -> PersistedHistoryEntry {
    PersistedHistoryEntry {
        id: entry.id,
        parent_id: entry.parent_id,
        edited_at: entry.edited_at,
        current: now == entry.id,
        selected_tip: tip == entry.id,
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
            .map(|edit| edit.to_persisted(|span| tree.text_for_span(span).to_string()))
            .collect(),
    }
}

#[derive(Clone)]
pub(crate) struct HistorySnapshot {
    entries: crate::app::domain::buffer::PersistentVector<PieceHistoryEntry>,
    tree: Arc<super::super::super::PieceTreeLite>,
    now: u64,
    tip: u64,
    last_edit: (u64, u64),
}
impl HistorySnapshot {
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }
    pub(crate) fn entry(&self, index: usize) -> PersistedHistoryEntry {
        export_entry(&self.entries[index], &self.tree, self.now, self.tip)
    }
    pub(crate) fn positions(&self) -> (u64, u64, u64, u64) {
        (self.now, self.tip, self.last_edit.0, self.last_edit.1)
    }
}
impl TextDocument {
    pub(crate) fn history_snapshot(&self) -> HistorySnapshot {
        HistorySnapshot {
            entries: self.history.entries.clone(),
            tree: self.content.piece_tree.clone(),
            now: self.history_now(),
            tip: self.history_tip(),
            last_edit: self.last_edit(),
        }
    }
}
