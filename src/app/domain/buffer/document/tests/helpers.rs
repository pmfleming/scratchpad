use super::{
    CharCursor, CursorRange, EditOperation, OperationRecord, PieceHistoryEntry, Range,
    TextDocument, TextDocumentOperationRecord,
};

pub(super) fn insert_edit(document: &mut TextDocument, start: usize, text: &str) {
    insert_edit_with_cursor(document, start, text, start, start + text.chars().count());
}

pub(super) fn insert_isolated_edit(document: &mut TextDocument, start: usize, text: &str) {
    document.history.latest_update_at = None;
    insert_edit(document, start, text);
}

pub(super) fn insert_edit_with_cursor(
    document: &mut TextDocument,
    start: usize,
    text: &str,
    previous_cursor: usize,
    next_cursor: usize,
) {
    document.insert_direct(start, text);
    document.push_edit_operation(OperationRecord {
        previous_cursor: cursor(previous_cursor),
        next_cursor: cursor(next_cursor),
        edits: vec![EditOperation {
            start_char: start,
            deleted_text: String::new(),
            inserted_text: text.to_owned(),
            deleted_spans: Vec::new(),
        }],
    });
}

pub(super) fn delete_edit(
    document: &mut TextDocument,
    range: Range<usize>,
    previous_cursor: usize,
    next_cursor: usize,
) {
    let deleted_text = document.piece_tree().extract_range(range.clone());
    let deleted_spans = document.byte_spans_for_range(range.clone());
    document.delete_char_range_direct(range.clone());
    document.push_edit_operation(OperationRecord {
        previous_cursor: cursor(previous_cursor),
        next_cursor: cursor(next_cursor),
        edits: vec![EditOperation {
            start_char: range.start,
            deleted_text,
            inserted_text: String::new(),
            deleted_spans,
        }],
    });
}

pub(super) fn history_record(document: &TextDocument) -> TextDocumentOperationRecord {
    entry_record(document, 0)
}

pub(super) fn entry_record(document: &TextDocument, index: usize) -> TextDocumentOperationRecord {
    document.operation_from_history_entry(&document.history_entries()[index])
}

pub(super) fn assert_history_byte_usage_consistent(document: &TextDocument) {
    let expected = document
        .history
        .entries
        .iter()
        .map(PieceHistoryEntry::byte_cost)
        .sum::<usize>();
    assert_eq!(document.history_byte_usage(), expected);
}

pub(super) fn cursor(index: usize) -> CursorRange {
    CursorRange::one(CharCursor::new(index))
}
