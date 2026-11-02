use super::{
    CursorRange, DocumentSnapshot, PieceSource, TextDocument, TextDocumentEditOperation,
    TextDocumentOperationRecord,
};

impl TextDocument {
    /// Record a historical preview as a new, standalone edit on the live branch.
    /// Preserve exact bytes (including mixed line endings), unlike typed insertion.
    pub(crate) fn commit_history_preview(
        &mut self,
        preview: &DocumentSnapshot,
        previous_selection: CursorRange,
    ) -> bool {
        let before = self.extract_text();
        let after = preview.extract_text();
        if before == after {
            return false;
        }
        let prefix = before
            .chars()
            .zip(after.chars())
            .take_while(|(a, b)| a == b)
            .count();
        let before_len = before.chars().count();
        let after_len = after.chars().count();
        let suffix = before
            .chars()
            .rev()
            .zip(after.chars().rev())
            .take((before_len - prefix).min(after_len - prefix))
            .take_while(|(a, b)| a == b)
            .count();
        let range = prefix..before_len - suffix;
        let deleted_text = before.chars().skip(prefix).take(range.len()).collect();
        let inserted_text: String = after
            .chars()
            .skip(prefix)
            .take(after_len - prefix - suffix)
            .collect();
        let deleted_spans = self.byte_spans_for_range(range.clone());
        self.history.latest_update_at = None;
        self.delete_char_range_direct(range);
        self.insert_direct_with_source(prefix, &inserted_text, PieceSource::Edit);
        self.push_operation_record(
            TextDocumentOperationRecord {
                previous_selection,
                next_selection: CursorRange::one(crate::app::domain::CharCursor::new(prefix)),
                edits: vec![TextDocumentEditOperation {
                    start_char: prefix,
                    deleted_text,
                    inserted_text,
                    deleted_spans,
                }],
            },
            PieceSource::Edit,
        );
        if let Some(entry) = self.history.entries.last_mut() {
            entry.summary = "Restore history state".into();
        }
        self.history.latest_update_at = None;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confirmation_is_one_exact_edit_and_undo_restores_the_original_branch() {
        let mut document = TextDocument::new("α\r\nold\n尾".into());
        document
            .replace_char_ranges_with_undo(
                &[(3..6, "current".into())],
                CursorRange::default(),
                CursorRange::default(),
            )
            .unwrap();
        let original = document.extract_text();
        let preview = TextDocument::new("α\npreview\r\n尾".into()).snapshot();
        assert!(document.commit_history_preview(&preview, CursorRange::default()));
        assert_eq!(document.history_entries().len(), 2);
        assert_eq!(document.extract_text(), "α\npreview\r\n尾");
        document.undo_last_operation().unwrap();
        assert_eq!(document.extract_text(), original);
        document.redo_last_operation().unwrap();
        assert_eq!(document.extract_text(), "α\npreview\r\n尾");
        assert!(!document.commit_history_preview(&preview, CursorRange::default()));
        assert_eq!(document.history_entries().len(), 2);
    }
}
