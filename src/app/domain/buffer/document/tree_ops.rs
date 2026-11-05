use super::super::{PieceHistoryEdits, PieceHistoryFlags, next_text_history_global_seq};
use super::{
    CursorRange, OperationDirection, PieceHistoryEdit, PieceHistoryEntry, PieceSource,
    TextDocument, TextDocumentEditOperation, TextDocumentOperationRecord, TextHistoryApplyError,
};

impl TextDocument {
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
            .find(|e| e.edited_at != 0)
            .map_or((0, 0), |e| (e.edited_at, e.global_seq))
    }

    pub(super) fn record_initial_content(&mut self) {
        let (edits, fingerprint) = self.piece_tree().initial_file_history().unwrap_or_else(|| {
            let mut start = 0;
            let edits: PieceHistoryEdits = self
                .byte_spans_for_range(0..self.piece_tree().len_chars())
                .into_iter()
                .map(|span| {
                    let edit = PieceHistoryEdit::Inserted {
                        start_char: start,
                        span,
                    };
                    start += self.piece_tree().text_for_span(span).chars().count() as u32;
                    edit
                })
                .collect();
            let fingerprint = self.fingerprint_for_history_edits(&edits);
            (edits, fingerprint)
        });
        if edits.is_empty() {
            return;
        }
        let entry = PieceHistoryEntry {
            id: self.history.next_id,
            parent_id: 0,
            edited_at: 0,
            global_seq: next_text_history_global_seq(),
            source: PieceSource::Load,
            visible_generation_before: 0,
            visible_generation_after: self.visible_generation(),
            fingerprint,
            summary: "Initial contents".into(),
            edits,
            flags: PieceHistoryFlags {
                undone: false,
                replayable: true,
                persisted: false,
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
                let record = self.replay_history_node(id, direction)?;
                selection = direction.selection(&record);
                latest_record = Some(record);
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
        self.history.pending_generation_before = None;
        self.seal_history_group();
        self.advance_history_revision();
        Ok(selection)
    }

    fn replay_history_node(
        &mut self,
        id: u64,
        direction: OperationDirection,
    ) -> Result<TextDocumentOperationRecord, TextHistoryApplyError> {
        let index = self
            .history
            .entries
            .binary_search_by_key(&id, |e| e.id)
            .map_err(|_| TextHistoryApplyError::OutOfBounds)?;
        if !self.history.entries[index].flags.replayable {
            return Err(TextHistoryApplyError::Conflict);
        }
        for edit in &self.history.entries[index].edits {
            for span in edit
                .deleted_spans()
                .iter()
                .copied()
                .chain(edit.inserted_span())
            {
                self.piece_tree()
                    .check_history_span(span)
                    .map_err(|_| TextHistoryApplyError::Conflict)?;
            }
        }
        let record = self.operation_from_history_entry(&self.history.entries[index]);
        let stored_edits = self.history.entries[index].edits.clone();
        let mut edits = record.edits.iter().zip(stored_edits.iter());
        while let Some((edit, stored)) = match direction {
            OperationDirection::Undo => edits.next_back(),
            OperationDirection::Redo => edits.next(),
        } {
            let expected = edit.expected_text(direction);
            let end = edit
                .start_char
                .checked_add(expected.chars().count())
                .ok_or(TextHistoryApplyError::OutOfBounds)?;
            if end > self.piece_tree().len_chars() {
                return Err(TextHistoryApplyError::OutOfBounds);
            }
            let range = edit.start_char..end;
            if self.piece_tree().extract_range(range.clone()) != expected {
                return Err(TextHistoryApplyError::Conflict);
            }
            let spans = match direction {
                OperationDirection::Undo => stored.deleted_spans().to_vec(),
                OperationDirection::Redo => stored.inserted_span().into_iter().collect(),
            };
            let tree = std::sync::Arc::make_mut(&mut self.content.piece_tree);
            tree.remove_char_range(range);
            tree.insert_history_spans(edit.start_char, &spans);
        }
        Ok(record)
    }

    pub(crate) fn replace_exact_with_history(&mut self, text: &str) {
        self.replace_exact_with_history_source(text, PieceSource::Load);
    }

    pub(crate) fn replace_exact_with_history_source(&mut self, text: &str, source: PieceSource) {
        let before = self.extract_text();
        if before == text {
            return;
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
    }
}
