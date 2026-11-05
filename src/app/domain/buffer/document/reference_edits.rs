//! Bulk edits retain references and small labels; deleted text is never flattened.
use super::super::{
    PieceHistoryEdits, PieceHistoryFlags, next_text_history_global_seq,
    normalize_inserted_text_line_endings,
};
use super::{
    CursorRange, PieceHistoryEdit, PieceHistoryEntry, PieceSource, TextDocument,
    TextDocumentEditOperation, TextDocumentOperationRecord, TextReplacementError, TextReplacements,
};
impl TextDocument {
    pub(crate) fn replace_document_references(
        &mut self,
        loaded: &Self,
        source: PieceSource,
    ) -> std::io::Result<()> {
        if self.piece_tree().read_fault().is_some() || loaded.piece_tree().read_fault().is_some() {
            return Err(std::io::Error::other("Replacement text is unreadable"));
        }
        if self.piece_tree().len_bytes() == loaded.piece_tree().len_bytes()
            && self.piece_tree().len_bytes() < 256 * 1024
            && self.try_content_digest()? == loaded.try_content_digest()?
        {
            return Ok(());
        }
        let before = self.content.clone();
        let deleted = self
            .piece_tree()
            .reference_spans(0..self.piece_tree().len_chars())?;
        let inserted = match self.piece_tree_mut().adopt_visible(loaded.piece_tree()) {
            Ok(spans) => spans,
            Err(cause) => {
                self.content = before;
                return Err(cause);
            }
        };
        let mut edits = PieceHistoryEdits::new();
        if !deleted.is_empty() {
            edits.push(PieceHistoryEdit::Deleted {
                start_char: 0,
                spans: deleted,
            });
        }
        let mut position = 0;
        for span in inserted {
            edits.push(PieceHistoryEdit::Inserted {
                start_char: position,
                span,
            });
            position += span.char_len;
        }
        if !edits.is_empty() {
            self.append_reference_entry(
                edits,
                CursorRange::default(),
                CursorRange::default(),
                source,
                if source == PieceSource::External {
                    "External changes loaded".into()
                } else {
                    "Loaded contents".into()
                },
            );
        }
        Ok(())
    }

    pub(super) fn replace_references(
        &mut self,
        replacements: TextReplacements<'_>,
        previous: CursorRange,
        next: CursorRange,
        source: PieceSource,
    ) -> Result<(), TextReplacementError> {
        let original = self.content.clone();
        let mut previews = Vec::new();
        let mut edits = PieceHistoryEdits::new();
        let result = (|| {
            for (range, text) in replacements {
                let deleted = self
                    .piece_tree()
                    .reference_spans(range.clone())
                    .map_err(|_| TextReplacementError::StorageUnreadable)?;
                let mut prefix = String::new();
                for span in self
                    .piece_tree()
                    .try_spans_for_range(range.start..range.end.min(range.start.saturating_add(80)))
                {
                    prefix.push_str(
                        &span
                            .map_err(|_| TextReplacementError::StorageUnreadable)?
                            .text,
                    );
                }
                let normalized =
                    normalize_inserted_text_line_endings(text, self.content.preferred_line_ending);
                let count = normalized.chars().count();
                let _pin = self
                    .piece_tree()
                    .pin_edit_boundaries(range.clone())
                    .map_err(|_| TextReplacementError::StorageUnreadable)?;
                self.piece_tree_mut().remove_char_range(range.clone());
                self.piece_tree_mut()
                    .insert_with_source(range.start, &normalized, source);
                if self.piece_tree().intake_error().is_some() {
                    return Err(TextReplacementError::BulkRefused);
                }
                if self.piece_tree().read_fault().is_some() {
                    return Err(TextReplacementError::StorageUnreadable);
                }
                let inserted = self
                    .piece_tree()
                    .reference_spans(range.start..range.start + count)
                    .map_err(|_| TextReplacementError::StorageUnreadable)?;
                let inserted = super::super::piece_tree::coalesce_spans(inserted);
                if !deleted.is_empty() {
                    edits.push(PieceHistoryEdit::Deleted {
                        start_char: range.start as u64,
                        spans: deleted,
                    });
                }
                let mut at = range.start as u64;
                for span in inserted {
                    edits.push(PieceHistoryEdit::Inserted {
                        start_char: at,
                        span,
                    });
                    at += span.char_len;
                }
                previews.push(TextDocumentEditOperation {
                    start_char: range.start,
                    deleted_text: prefix,
                    inserted_text: normalized.chars().take(80).collect(),
                    deleted_spans: Vec::new(),
                });
            }
            Ok(())
        })();
        if result.is_err() {
            self.content = original;
            return result;
        }
        if !edits.is_empty() {
            let summary = super::super::history::operation_summary(
                source,
                &TextDocumentOperationRecord {
                    previous_selection: previous,
                    next_selection: next,
                    edits: previews,
                },
            );
            self.append_reference_entry(edits, previous, next, source, summary);
        }
        Ok(())
    }
    pub(super) fn append_reference_entry(
        &mut self,
        edits: PieceHistoryEdits,
        previous: CursorRange,
        next: CursorRange,
        source: PieceSource,
        summary: String,
    ) {
        let entry = PieceHistoryEntry {
            id: self.history.next_id,
            parent_id: self.history.tree.now(),
            edited_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            global_seq: next_text_history_global_seq(),
            source,
            summary,
            edits,
            flags: PieceHistoryFlags {
                undone: false,
                replayable: true,
            },
            previous_selection: previous,
            next_selection: next,
        };
        self.history.next_id = self.history.next_id.saturating_add(1);
        self.history.byte_usage += entry.byte_cost();
        assert!(self.history.tree.append(entry.id));
        self.history.entries.push(entry);
        self.history.latest_operation_record = None;
        self.seal_history_group();
        self.advance_history_revision();
    }
}
