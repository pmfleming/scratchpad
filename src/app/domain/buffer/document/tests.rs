use super::super::history::TEXT_HISTORY_SOFT_DIVIDER_PAUSE;
use super::*;
use crate::app::ui::editor_content::native_editor::{CharCursor, EditOperation};
use std::time::Duration;

macro_rules! empty_document {
    () => {
        TextDocument::new(String::new())
    };
}

macro_rules! insert_sequence {
    ($document:expr, $text:expr) => {
        for (offset, ch) in $text.chars().enumerate() {
            insert_edit($document, offset, &ch.to_string());
        }
    };
}

macro_rules! insert_isolated_sequence {
    ($document:expr, $text:expr) => {
        for (offset, ch) in $text.chars().enumerate() {
            insert_isolated_edit($document, offset, &ch.to_string());
        }
    };
}

macro_rules! assert_entry_inserted_text {
    ($document:expr, $index:expr, $text:expr) => {
        assert_eq!(
            entry_record($document, $index).edits[0].inserted_text,
            $text
        );
    };
}

macro_rules! assert_single_entry_insert {
    ($document:expr, $text:expr) => {
        assert_eq!($document.extract_text(), $text);
        assert_eq!($document.operation_undo_depth(), 1);
        assert_entry_inserted_text!($document, 0, $text);
    };
}

macro_rules! assert_undo_restores_text {
    ($document:expr, $expected:expr) => {
        $document.undo_last_operation();
        assert_eq!($document.extract_text(), $expected);
    };
}

#[test]
fn adjacent_typing_coalesces_into_one_undo_entry() {
    let mut document = empty_document!();

    insert_sequence!(&mut document, "abc");

    assert_single_entry_insert!(&document, "abc");
    assert_undo_restores_text!(&mut document, "");
}

#[test]
fn backspace_inside_typing_burst_shrinks_the_insert_entry() {
    let mut document = empty_document!();

    insert_sequence!(&mut document, "abc");
    delete_edit(&mut document, 2..3, 3, 2);
    insert_edit(&mut document, 2, "d");

    assert_single_entry_insert!(&document, "abd");
    assert_undo_restores_text!(&mut document, "");
}

#[test]
fn mistype_delete_retype_drops_the_transient_mistype() {
    let mut document = empty_document!();

    insert_edit(&mut document, 0, "x");
    delete_edit(&mut document, 0..1, 1, 0);
    insert_edit(&mut document, 0, "y");

    assert_single_entry_insert!(&document, "y");
    assert_undo_restores_text!(&mut document, "");
}

#[test]
fn removed_transient_edit_does_not_reopen_previous_coalescing_window() {
    let mut document = empty_document!();

    insert_edit(&mut document, 0, "a");
    document.latest_history_update_at = None;
    insert_edit(&mut document, 1, "x");
    delete_edit(&mut document, 1..2, 2, 1);
    insert_edit(&mut document, 1, "y");

    assert_eq!(document.extract_text(), "ay");
    assert_eq!(document.operation_undo_depth(), 2);
    assert_entry_inserted_text!(&document, 0, "a");
    assert_entry_inserted_text!(&document, 1, "y");
}

#[test]
fn adjacent_backspaces_coalesce_into_one_delete_entry() {
    let mut document = TextDocument::new("abcd".to_owned());

    delete_edit(&mut document, 3..4, 4, 3);
    delete_edit(&mut document, 2..3, 3, 2);
    delete_edit(&mut document, 1..2, 2, 1);

    assert_eq!(document.extract_text(), "a");
    assert_eq!(document.operation_undo_depth(), 1);
    let record = history_record(&document);
    assert_eq!(record.edits[0].start_char, 1);
    assert_eq!(record.edits[0].deleted_text, "bcd");

    document.undo_last_operation();
    assert_eq!(document.extract_text(), "abcd");
}

#[test]
fn long_typing_burst_stays_one_entry() {
    let mut document = empty_document!();
    let phrase = "highlighting should be consistent";

    insert_sequence!(&mut document, phrase);

    assert_single_entry_insert!(&document, phrase);
}

#[test]
fn hard_divider_seals_the_entry() {
    let mut document = empty_document!();

    insert_sequence!(&mut document, "Hi.Bye");

    assert_eq!(document.extract_text(), "Hi.Bye");
    assert_eq!(document.operation_undo_depth(), 2);
    assert_entry_inserted_text!(&document, 0, "Hi.");
    assert_entry_inserted_text!(&document, 1, "Bye");
}

#[test]
fn newline_seals_the_entry() {
    let mut document = empty_document!();

    insert_edit(&mut document, 0, "a");
    insert_edit(&mut document, 1, "\n");
    insert_edit(&mut document, 2, "b");

    assert_eq!(document.extract_text(), "a\nb");
    assert_eq!(document.operation_undo_depth(), 2);
}

#[test]
fn soft_divider_does_not_seal_inside_a_continuous_burst() {
    let mut document = empty_document!();

    insert_sequence!(&mut document, "Hi, you");

    assert_single_entry_insert!(&document, "Hi, you");
}

#[test]
fn soft_divider_seals_after_a_pause() {
    let mut document = empty_document!();

    insert_sequence!(&mut document, "Hi,");
    // Simulate the user pausing past the soft-divider seal threshold.
    document.latest_history_update_at =
        Some(Instant::now() - TEXT_HISTORY_SOFT_DIVIDER_PAUSE - Duration::from_millis(50));
    insert_edit(&mut document, 3, " ");

    assert_eq!(document.extract_text(), "Hi, ");
    assert_eq!(document.operation_undo_depth(), 2);
    assert_entry_inserted_text!(&document, 0, "Hi,");
}

#[test]
fn prefer_next_row_flip_does_not_split_a_typing_burst() {
    let mut document = TextDocument::new(String::new());

    document.insert_direct(0, "a");
    document.push_edit_operation(OperationRecord {
        previous_cursor: cursor(0),
        next_cursor: cursor(1),
        edits: vec![EditOperation {
            start_char: 0,
            deleted_text: String::new(),
            inserted_text: "a".to_owned(),
            deleted_spans: Vec::new(),
        }],
    });

    // Same caret position, but the editor reports it with prefer_next_row=true
    // (e.g., the caret sits at the end of a soft-wrapped line).
    document.insert_direct(1, "b");
    let prev_with_flip = CursorRange::one(CharCursor {
        index: 1,
        prefer_next_row: true,
    });
    document.push_edit_operation(OperationRecord {
        previous_cursor: prev_with_flip,
        next_cursor: cursor(2),
        edits: vec![EditOperation {
            start_char: 1,
            deleted_text: String::new(),
            inserted_text: "b".to_owned(),
            deleted_spans: Vec::new(),
        }],
    });

    assert_eq!(document.extract_text(), "ab");
    assert_eq!(document.operation_undo_depth(), 1);
    assert_eq!(history_record(&document).edits[0].inserted_text, "ab");
}

#[test]
fn dash_is_a_soft_divider() {
    let mut document = empty_document!();

    // A continuous burst through the dash stays in one entry.
    insert_sequence!(&mut document, "well-known");
    assert_eq!(document.operation_undo_depth(), 1);
    assert_entry_inserted_text!(&document, 0, "well-known");

    // A burst that ends on a dash, then a pause, seals the entry.
    let mut other = empty_document!();
    insert_sequence!(&mut other, "well-");
    other.latest_history_update_at =
        Some(Instant::now() - TEXT_HISTORY_SOFT_DIVIDER_PAUSE - Duration::from_millis(50));
    insert_edit(&mut other, 5, "k");
    assert_eq!(other.operation_undo_depth(), 2);
}

#[test]
fn cursor_jump_starts_a_new_undo_entry() {
    let mut document = TextDocument::new(String::new());

    insert_edit(&mut document, 0, "a");
    insert_edit_with_cursor(&mut document, 0, "b", 0, 1);

    assert_eq!(document.extract_text(), "ba");
    assert_eq!(document.operation_undo_depth(), 2);
}

#[test]
fn keyboard_redo_replays_one_history_entry_at_a_time() {
    let mut document = empty_document!();
    insert_isolated_sequence!(&mut document, "abc");

    document.undo_last_operation();
    document.undo_last_operation();

    assert_eq!(document.extract_text(), "a");
    assert_eq!(document.operation_redo_depth(), 2);

    document.redo_last_operation();

    assert_eq!(document.extract_text(), "ab");
    assert_eq!(document.operation_undo_depth(), 2);
    assert_eq!(document.operation_redo_depth(), 1);

    document.redo_last_operation();

    assert_eq!(document.extract_text(), "abc");
    assert_eq!(document.operation_undo_depth(), 3);
    assert_eq!(document.operation_redo_depth(), 0);
}

#[test]
fn target_history_redo_replays_through_the_clicked_entry() {
    let mut document = empty_document!();
    insert_isolated_sequence!(&mut document, "abc");
    let clicked_entry = document.history_entries()[2].id;

    document.undo_last_operation();
    document.undo_last_operation();

    document.apply_text_history_redo(clicked_entry).unwrap();

    assert_eq!(document.extract_text(), "abc");
    assert_eq!(document.operation_undo_depth(), 3);
    assert_eq!(document.operation_redo_depth(), 0);
}

#[test]
fn target_history_undo_replays_clicked_entry_and_later_entries() {
    let mut document = empty_document!();
    insert_isolated_sequence!(&mut document, "abc");
    let clicked_entry = document.history_entries()[1].id;

    document.apply_text_history_undo(clicked_entry).unwrap();

    assert_eq!(document.extract_text(), "a");
    assert_eq!(document.operation_undo_depth(), 1);
    assert_eq!(document.operation_redo_depth(), 2);
}

fn insert_edit(document: &mut TextDocument, start: usize, text: &str) {
    insert_edit_with_cursor(document, start, text, start, start + text.chars().count());
}

fn insert_isolated_edit(document: &mut TextDocument, start: usize, text: &str) {
    document.latest_history_update_at = None;
    insert_edit(document, start, text);
}

fn insert_edit_with_cursor(
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

fn delete_edit(
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

fn history_record(document: &TextDocument) -> TextDocumentOperationRecord {
    entry_record(document, 0)
}

fn entry_record(document: &TextDocument, index: usize) -> TextDocumentOperationRecord {
    document.operation_from_history_entry(&document.history_entries()[index])
}

fn cursor(index: usize) -> CursorRange {
    CursorRange::one(CharCursor::new(index))
}
