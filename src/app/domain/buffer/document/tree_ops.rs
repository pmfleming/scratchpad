use super::super::{PieceHistoryEdits, PieceHistoryFlags, next_text_history_global_seq};
use super::{
    CursorRange, OperationDirection, PieceHistoryEdit, PieceHistoryEntry, PieceSource,
    TextDocument, TextDocumentEditOperation, TextDocumentOperationRecord, TextHistoryApplyError,
};

impl TextDocument {
    pub(crate) fn history_step_target(&self, undo: bool) -> Option<u64> {
        if undo {
            self.history.tree.parent(self.history.tree.now())
        } else {
            self.history.tree.redo_target()
        }
    }
    pub(crate) fn history_needs_preparation(&self, target: u64) -> bool {
        let Some(transition) = self.history.tree.navigation(target) else {
            return false;
        };
        if transition.undo.len() + transition.redo.len() > 256 {
            return true;
        }
        let mut cost = 0usize;
        for id in transition.undo.iter().chain(&transition.redo) {
            if let Ok(index) = self.history.entries.binary_search_by_key(id, |e| e.id) {
                let entry = &self.history.entries[index];
                let mut spans = 0usize;
                if entry.edits.iter().any(|edit| {
                    spans = spans.saturating_add(match edit {
                        PieceHistoryEdit::Inserted { .. } => 1,
                        PieceHistoryEdit::Deleted { spans, .. } => spans.len(),
                        PieceHistoryEdit::Replaced { deleted, .. } => deleted.len() + 1,
                    });
                    spans > 256
                }) {
                    return true;
                }
                cost = cost.saturating_add(entry.byte_cost());
                if cost >= 256 * 1024 {
                    return true;
                }
            }
        }
        false
    }

    #[must_use]
    pub fn history_now(&self) -> u64 {
        self.history.tree.now()
    }
    #[must_use]
    pub fn history_tip(&self) -> u64 {
        self.history.tree.tip()
    }
    #[must_use]
    pub fn history_route(&self) -> &[u64] {
        self.history.tree.route()
    }
    pub(crate) fn history_structure_key(&self) -> (u64, usize, u64) {
        (
            self.identity,
            self.history.entries.len(),
            self.history.entries.last().map_or(0, |entry| entry.id),
        )
    }
    pub(crate) fn is_on_history_route(&self, node: u64) -> bool {
        self.history.tree.is_on_route(node)
    }

    pub(crate) fn is_history_node_applied(&self, node: u64) -> bool {
        self.history.tree.is_applied(node)
    }

    pub fn seal_history_group(&mut self) {
        self.history.latest_update_at = None;
    }

    #[must_use]
    pub fn last_edit(&self) -> (u64, u64) {
        self.history
            .entries
            .iter()
            .rev()
            .find(|e| e.summary != "Initial contents")
            .map_or((0, 0), |e| (e.edited_at, e.global_seq))
    }

    pub(super) fn record_initial_content(&mut self) {
        let mut start = 0;
        let edits: PieceHistoryEdits = self
            .byte_spans_for_range(0..self.piece_tree().len_chars())
            .into_iter()
            .map(|span| {
                let edit = PieceHistoryEdit::Inserted {
                    start_char: start,
                    span,
                };
                start += span.char_len;
                edit
            })
            .collect();
        if edits.is_empty() {
            return;
        }
        let entry = PieceHistoryEntry {
            id: self.history.next_id,
            parent_id: 0,
            edited_at: 0,
            global_seq: next_text_history_global_seq(),
            source: PieceSource::Load,

            summary: "Initial contents".into(),
            edits,
            flags: PieceHistoryFlags {
                undone: false,
                replayable: true,
            },
            previous_selection: CursorRange::default(),
            next_selection: CursorRange::default(),
        };
        self.history.next_id += 1;
        self.history.byte_usage += entry.byte_cost();
        assert!(self.history.tree.append(entry.id));
        self.history.entries.push(entry);
        self.advance_history_revision();
    }

    pub(super) fn refresh_tree_flags(&mut self) {
        let applied: std::collections::HashSet<_> = self.history.tree.route()
            [..self.history.tree.position()]
            .iter()
            .copied()
            .collect();
        for index in 0..self.history.entries.len() {
            let entry = &mut self.history.entries[index];
            entry.flags.undone = !applied.contains(&entry.id);
        }
    }

    /// Replay into copy-on-write content and publish the position only on success.
    /// The history tree and all branch payloads remain untouched during validation.
    pub(crate) fn navigate_history(
        &mut self,
        target: u64,
    ) -> Result<CursorRange, TextHistoryApplyError> {
        let transition = self
            .history
            .tree
            .navigation(target)
            .ok_or(TextHistoryApplyError::OutOfBounds)?;
        if target == self.history.tree.now() {
            return Ok(self
                .history
                .entries
                .binary_search_by_key(&target, |e| e.id)
                .ok()
                .map_or(CursorRange::default(), |index| {
                    self.history.entries[index].next_selection
                }));
        }
        let original = self.content.clone();
        let mut selection = CursorRange::default();
        let mut latest_record = None;
        let result = (|| {
            for (id, direction) in transition
                .undo
                .iter()
                .map(|id| (*id, OperationDirection::Undo))
                .chain(
                    transition
                        .redo
                        .iter()
                        .map(|id| (*id, OperationDirection::Redo)),
                )
            {
                let (next, record) = self.replay_history_node(id, direction)?;
                selection = next;
                latest_record = record;
            }
            Ok(())
        })();
        if let Err(error) = result {
            self.content = original;
            return Err(error);
        }
        for &id in &transition.undo {
            if let Ok(index) = self.history.entries.binary_search_by_key(&id, |e| e.id) {
                self.history.entries[index].flags.undone = true;
            }
        }
        for &id in &transition.redo {
            if let Ok(index) = self.history.entries.binary_search_by_key(&id, |e| e.id) {
                self.history.entries[index].flags.undone = false;
            }
        }
        self.history.tree.commit(transition);
        self.history.latest_operation_record = latest_record;
        self.seal_history_group();
        self.advance_history_revision();
        Ok(selection)
    }

    fn replay_history_node(
        &mut self,
        id: u64,
        direction: OperationDirection,
    ) -> Result<(CursorRange, Option<TextDocumentOperationRecord>), TextHistoryApplyError> {
        let index = self
            .history
            .entries
            .binary_search_by_key(&id, |e| e.id)
            .map_err(|_| TextHistoryApplyError::OutOfBounds)?;
        let entry = &self.history.entries[index];
        if !entry.flags.replayable {
            return Err(TextHistoryApplyError::Conflict);
        }
        let selection = match direction {
            OperationDirection::Undo => entry.previous_selection,
            OperationDirection::Redo => entry.next_selection,
        };
        let record = if entry.byte_cost() <= 256 * 1024 {
            let _pin = self
                .piece_tree()
                .pin_history_spans(entry.edits.iter().flat_map(PieceHistoryEdit::spans))
                .map_err(|_| TextHistoryApplyError::Conflict)?;
            Some(self.operation_from_history_entry(entry))
        } else {
            None
        };
        let stored = entry.edits.clone();
        let mut edits = stored.iter();
        while let Some(edit) = match direction {
            OperationDirection::Undo => edits.next_back(),
            OperationDirection::Redo => edits.next(),
        } {
            let inserted = edit.inserted_span().into_iter().collect::<Vec<_>>();
            let (expected, replacement) = match direction {
                OperationDirection::Undo => (inserted.as_slice(), edit.deleted_spans()),
                OperationDirection::Redo => (edit.deleted_spans(), inserted.as_slice()),
            };
            let start = usize::try_from(edit.start_char())
                .map_err(|_| TextHistoryApplyError::OutOfBounds)?;
            let count = expected
                .iter()
                .try_fold(0usize, |n, s| {
                    usize::try_from(s.char_len)
                        .ok()
                        .and_then(|v| n.checked_add(v))
                })
                .ok_or(TextHistoryApplyError::OutOfBounds)?;
            let end = start
                .checked_add(count)
                .ok_or(TextHistoryApplyError::OutOfBounds)?;
            if end > self.piece_tree().len_chars() {
                return Err(TextHistoryApplyError::OutOfBounds);
            }
            if !self
                .piece_tree()
                .matches_history_range(start..end, expected)
                .map_err(|_| TextHistoryApplyError::Conflict)?
            {
                return Err(TextHistoryApplyError::Conflict);
            }
            let _pin = self
                .piece_tree()
                .pin_edit_boundaries(start..end)
                .map_err(|_| TextHistoryApplyError::Conflict)?;
            let tree = std::sync::Arc::make_mut(&mut self.content.piece_tree);
            tree.remove_char_range(start..end);
            tree.insert_history_spans(start, replacement)
                .map_err(|_| TextHistoryApplyError::Conflict)?;
        }
        Ok((selection, record))
    }

    pub(crate) fn replace_exact_with_history(&mut self, text: &str) {
        self.replace_exact_with_history_source(text, PieceSource::Load);
    }

    /// The loaded state becomes installed after its Opened observation. Updating
    /// only ordering metadata keeps the stable node identity and its read timestamp.
    pub(crate) fn order_external_edit_after_open(&mut self, node: u64) {
        let Some(index) = self.history.entries.len().checked_sub(1) else {
            return;
        };
        let entry = &mut self.history.entries[index];
        if entry.id == node && entry.source == PieceSource::External {
            entry.global_seq = next_text_history_global_seq();
            self.advance_history_revision();
        }
    }

    pub(crate) fn replace_exact_with_history_source(
        &mut self,
        text: &str,
        source: PieceSource,
    ) -> bool {
        if self.piece_tree().read_fault().is_some() {
            return false;
        }
        let Ok(_pin) = self
            .piece_tree()
            .pin_range(0..self.piece_tree().len_chars())
        else {
            return false;
        };
        let before = self.extract_text();
        if before == text {
            return true;
        }
        self.seal_history_group();
        let deleted_spans = self.byte_spans_for_range(0..self.piece_tree().len_chars());
        self.delete_char_range_direct(0..self.piece_tree().len_chars());
        self.insert_direct_with_source(0, text, source);
        self.push_operation_record(
            TextDocumentOperationRecord {
                previous_selection: CursorRange::default(),
                next_selection: CursorRange::default(),
                edits: vec![TextDocumentEditOperation {
                    start_char: 0,
                    deleted_text: before,
                    inserted_text: text.into(),
                    deleted_spans,
                }],
            },
            source,
        );
        self.seal_history_group();
        true
    }
}
