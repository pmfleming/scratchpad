mod export;
use super::super::super::history::{persist_cursor_range, restore_cursor_range};
use super::super::super::{
    PersistedHistoryEdit, PersistedHistoryEntry, PieceHistoryEdit, PieceHistoryEdits,
    PieceHistoryEntry, PieceSource, register_text_history_global_seq,
};
use super::super::TextDocument;
use std::sync::Arc;

impl TextDocument {
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
        fingerprint: super::super::super::fingerprint_parts(entry.edits.iter().flat_map(|edit| {
            let deleted = (!edit.deleted_spans().is_empty()).then(|| {
                edit.deleted_spans()
                    .iter()
                    .map(|s| tree.text_for_span(*s).to_string())
                    .collect::<String>()
            });
            let inserted = edit
                .inserted_span()
                .map(|s| tree.text_for_span(s).to_string());
            deleted.into_iter().chain(inserted)
        })),
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
    pub(crate) fn copy_to_store(
        &self,
        copy: &mut crate::app::services::chunk_store::Rebind,
    ) -> std::io::Result<Self> {
        let mut snapshot = self.clone();
        snapshot.tree = Arc::new(self.tree.copy_to_store(copy)?);
        Ok(snapshot)
    }
    pub(crate) fn snapshot(&self) -> crate::app::domain::DocumentSnapshot {
        crate::app::domain::DocumentSnapshot::from_shared(self.tree.clone())
    }

    pub(crate) fn reference_entry(&self, index: usize) -> &PieceHistoryEntry {
        &self.entries[index]
    }
    pub(crate) fn piece_tree(&self) -> &super::super::super::PieceTreeLite {
        &self.tree
    }
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }
    pub(crate) fn entry(&self, index: usize) -> std::io::Result<PersistedHistoryEntry> {
        let _pin = self.tree.pin_history_spans(
            self.entries[index]
                .edits
                .iter()
                .flat_map(PieceHistoryEdit::spans),
        )?;
        Ok(export_entry(
            &self.entries[index],
            &self.tree,
            self.now,
            self.tip,
        ))
    }
    pub(crate) fn positions(&self) -> (u64, u64, u64, u64) {
        (self.now, self.tip, self.last_edit.0, self.last_edit.1)
    }
}
impl TextDocument {
    /// Repository reconstruction checks graph and storage ranges without loading
    /// any immutable bodies or replaying the route to Now.
    pub(crate) fn from_references(
        piece_tree: super::super::super::PieceTreeLite,
        entries: Vec<PieceHistoryEntry>,
        now: u64,
        tip: u64,
    ) -> std::io::Result<Self> {
        let topology = super::super::super::edit_tree::EditTree::restore(
            entries.iter().map(|e| (e.id, e.parent_id)),
            now,
            tip,
        )
        .ok_or_else(|| std::io::Error::other("invalid history topology"))?;
        if entries
            .iter()
            .flat_map(|e| &e.edits)
            .flat_map(PieceHistoryEdit::spans)
            .any(|s| !piece_tree.contains_history_span(s))
        {
            return Err(std::io::Error::other("history span exceeds stored text"));
        }
        let mut document = Self::new(String::new());
        document.content.piece_tree = Arc::new(piece_tree);
        document.history.next_id = entries.last().map_or(1, |e| e.id.saturating_add(1));
        for entry in entries {
            register_text_history_global_seq(entry.global_seq);
            document.history.byte_usage += entry.byte_cost();
            document.history.entries.push(entry);
        }
        document.history.tree = topology;
        document.refresh_tree_flags();
        document.advance_history_revision();
        Ok(document)
    }
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
