use super::super::history::TEXT_HISTORY_SOFT_DIVIDER_PAUSE;
use super::{
    PieceHistoryEntry, PieceSource, TextDocument, TextDocumentEditOperation,
    TextDocumentOperationRecord, TextHistoryApplyError, TextHistoryBudget,
};
use crate::app::ui::editor_content::native_editor::{
    CharCursor, CursorRange, EditOperation, OperationRecord,
};
use std::ops::Range;
use std::time::{Duration, Instant};

mod generation;
mod helpers;
use helpers::*;

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
    document.history.latest_update_at = None;
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
    document.history.latest_update_at =
        Instant::now().checked_sub(TEXT_HISTORY_SOFT_DIVIDER_PAUSE + Duration::from_millis(50));
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
    other.history.latest_update_at =
        Instant::now().checked_sub(TEXT_HISTORY_SOFT_DIVIDER_PAUSE + Duration::from_millis(50));
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

#[test]
fn multi_edit_history_undo_and_redo_apply_in_safe_order() {
    let mut document = TextDocument::new("one two one".to_owned());

    document
        .replace_char_ranges_with_undo(
            &[(8..11, "1".to_owned()), (0..3, "1".to_owned())],
            cursor(0),
            cursor(9),
        )
        .unwrap();

    assert_eq!(document.extract_text(), "1 two 1");
    document.undo_last_operation();
    assert_eq!(document.extract_text(), "one two one");
    document.redo_last_operation();
    assert_eq!(document.extract_text(), "1 two 1");
}

#[test]
fn target_history_undo_rejects_when_current_text_no_longer_matches_entry() {
    let mut document = empty_document!();
    insert_isolated_sequence!(&mut document, "abc");
    let entry_id = document.history_entries()[1].id;
    document.delete_char_range_internal(1..2);
    document.insert_raw_text("x", 1);

    let result = document.apply_text_history_undo(entry_id);

    assert_eq!(result, Err(TextHistoryApplyError::Conflict));
    assert_eq!(document.extract_text(), "axc");
    assert_eq!(document.operation_undo_depth(), 3);
}

#[test]
fn undo_then_typing_clears_redo_without_coalescing_into_survivor() {
    let mut document = empty_document!();
    insert_isolated_sequence!(&mut document, "abc");

    document.undo_last_operation();
    insert_edit(&mut document, 2, "x");

    assert_eq!(document.extract_text(), "abx");
    assert_eq!(document.operation_undo_depth(), 3);
    assert_eq!(document.operation_redo_depth(), 0);
    assert_entry_inserted_text!(&document, 1, "b");
    assert_entry_inserted_text!(&document, 2, "x");
}

#[test]
fn redo_then_typing_starts_a_new_coalescing_boundary() {
    let mut document = empty_document!();
    insert_isolated_sequence!(&mut document, "ab");

    document.undo_last_operation();
    document.redo_last_operation();
    insert_edit(&mut document, 2, "c");

    assert_eq!(document.extract_text(), "abc");
    assert_eq!(document.operation_undo_depth(), 3);
    assert_entry_inserted_text!(&document, 1, "b");
    assert_entry_inserted_text!(&document, 2, "c");
}

#[test]
fn empty_replacement_does_not_enter_history_or_clear_redo() {
    let mut document = empty_document!();
    insert_isolated_sequence!(&mut document, "ab");
    document.undo_last_operation();
    let revision_before = document.history_revision_counter();

    document
        .replace_char_ranges_with_undo(&[(1..1, String::new())], cursor(1), cursor(1))
        .unwrap();

    assert_eq!(document.extract_text(), "a");
    assert_eq!(document.history_revision_counter(), revision_before);
    assert_eq!(document.operation_undo_depth(), 1);
    assert_eq!(document.operation_redo_depth(), 1);
    assert!(document.latest_operation_record().is_some());
}

#[test]
fn no_op_edits_are_dropped_from_multi_edit_records() {
    let mut document = TextDocument::new("ab".to_owned());
    let before = document.visible_generation();
    document.capture_pending_history_generation_before();
    document.delete_char_range_direct(1..2);
    document.push_operation_record(
        TextDocumentOperationRecord {
            previous_selection: cursor(2),
            next_selection: cursor(1),
            edits: vec![
                TextDocumentEditOperation {
                    start_char: 0,
                    deleted_text: String::new(),
                    inserted_text: String::new(),
                    deleted_spans: Vec::new(),
                },
                TextDocumentEditOperation {
                    start_char: 1,
                    deleted_text: "b".to_owned(),
                    inserted_text: String::new(),
                    deleted_spans: Vec::new(),
                },
            ],
        },
        PieceSource::SearchReplace,
    );

    assert_eq!(document.extract_text(), "a");
    assert_eq!(document.operation_undo_depth(), 1);
    let record = history_record(&document);
    assert_eq!(record.edits.len(), 1);
    assert_eq!(record.edits[0].deleted_text, "b");
    assert_eq!(
        document.history_entries()[0].visible_generation_before,
        before
    );
}

#[test]
fn per_file_entry_limit_evicts_oldest_entries() {
    let mut document = empty_document!();
    document.set_history_budget(TextHistoryBudget {
        per_file_entry_limit: 100,
        per_file_byte_budget: 64 * 1024 * 1024,
        aggregate_byte_budget: 64 * 1024 * 1024,
        persisted_payload_budget: 64 * 1024 * 1024,
        derived_from_memory: false,
    });

    for index in 0..105 {
        document.history.latest_update_at = None;
        insert_edit(&mut document, index, "x");
    }

    assert_eq!(document.history_entries().len(), 100);
    assert_eq!(document.operation_undo_depth(), 100);
    assert_eq!(document.extract_text(), "x".repeat(105));
    let first_record = entry_record(&document, 0);
    assert_eq!(first_record.edits[0].start_char, 5);
}

#[test]
fn budget_shrink_eviction_bumps_history_revision() {
    let mut document = empty_document!();
    insert_isolated_sequence!(&mut document, &"x".repeat(105));
    let revision_before = document.history_revision_counter();

    document.set_history_budget(TextHistoryBudget {
        per_file_entry_limit: 100,
        per_file_byte_budget: 64 * 1024 * 1024,
        aggregate_byte_budget: 64 * 1024 * 1024,
        persisted_payload_budget: 64 * 1024 * 1024,
        derived_from_memory: false,
    });

    assert_eq!(document.history_entries().len(), 100);
    assert_ne!(document.history_revision_counter(), revision_before);
}

#[test]
fn compaction_keeps_retained_history_replayable() {
    let mut document = empty_document!();
    insert_isolated_sequence!(&mut document, "abc");

    document.drop_oldest_history_entry();

    assert_eq!(document.extract_text(), "abc");
    document.undo_last_operation();
    assert_eq!(document.extract_text(), "ab");
    document.redo_last_operation();
    assert_eq!(document.extract_text(), "abc");
}

#[test]
fn cached_history_byte_usage_tracks_history_mutations() {
    let mut document = empty_document!();
    assert_history_byte_usage_consistent(&document);

    insert_sequence!(&mut document, "abc");
    assert_history_byte_usage_consistent(&document);

    delete_edit(&mut document, 2..3, 3, 2);
    assert_history_byte_usage_consistent(&document);

    document.undo_last_operation();
    insert_isolated_edit(&mut document, 0, "z");
    assert_history_byte_usage_consistent(&document);

    document.set_history_budget(TextHistoryBudget {
        per_file_entry_limit: 1,
        per_file_byte_budget: 64 * 1024 * 1024,
        aggregate_byte_budget: 64 * 1024 * 1024,
        persisted_payload_budget: 64 * 1024 * 1024,
        derived_from_memory: false,
    });
    assert_history_byte_usage_consistent(&document);

    let exported = document.exported_history();
    let mut restored = TextDocument::new(document.extract_text());
    restored.restore_exported_history(exported);
    assert_history_byte_usage_consistent(&restored);

    restored.drop_oldest_history_entry();
    assert_history_byte_usage_consistent(&restored);

    restored.clear_operation_history();
    assert_history_byte_usage_consistent(&restored);
}

#[test]
fn backspace_across_hard_dividers_starts_new_entries() {
    let mut document = TextDocument::new("a.b".to_owned());

    delete_edit(&mut document, 2..3, 3, 2);
    delete_edit(&mut document, 1..2, 2, 1);
    delete_edit(&mut document, 0..1, 1, 0);

    assert_eq!(document.extract_text(), "");
    assert_eq!(document.operation_undo_depth(), 3);
    assert_eq!(entry_record(&document, 0).edits[0].deleted_text, "b");
    assert_eq!(entry_record(&document, 1).edits[0].deleted_text, ".");
    assert_eq!(entry_record(&document, 2).edits[0].deleted_text, "a");
}

#[test]
fn forward_delete_across_hard_divider_starts_new_entries() {
    let mut document = TextDocument::new("a.b".to_owned());

    delete_edit(&mut document, 0..1, 0, 0);
    delete_edit(&mut document, 0..1, 0, 0);
    delete_edit(&mut document, 0..1, 0, 0);

    assert_eq!(document.extract_text(), "");
    assert_eq!(document.operation_undo_depth(), 3);
    assert_eq!(entry_record(&document, 0).edits[0].deleted_text, "a");
    assert_eq!(entry_record(&document, 1).edits[0].deleted_text, ".");
    assert_eq!(entry_record(&document, 2).edits[0].deleted_text, "b");
}

#[test]
fn adjacent_pre_burst_backspace_coalesces_as_replacement() {
    let mut document = TextDocument::new("z".to_owned());

    insert_edit(&mut document, 1, "a");
    delete_edit(&mut document, 0..1, 2, 0);

    assert_eq!(document.extract_text(), "a");
    assert_eq!(document.operation_undo_depth(), 1);
    let record = history_record(&document);
    assert_eq!(record.edits[0].start_char, 0);
    assert_eq!(record.edits[0].deleted_text, "z");
    assert_eq!(record.edits[0].inserted_text, "a");

    document.undo_last_operation();
    assert_eq!(document.extract_text(), "z");
    document.redo_last_operation();
    assert_eq!(document.extract_text(), "a");
}

#[test]
fn imported_history_keeps_only_contiguous_undone_suffix_redoable() {
    let mut source = empty_document!();
    insert_isolated_sequence!(&mut source, "abc");
    source.history.entries[0].flags.undone = true;
    source.history.entries[1].flags.undone = false;
    source.history.entries[2].flags.undone = true;
    let exported = source.exported_history();

    let mut restored = TextDocument::new("abc".to_owned());
    restored.restore_exported_history(exported);

    assert!(!restored.history_entries()[0].is_undone());
    assert!(!restored.history_entries()[0].flags.replayable);
    assert!(!restored.history_entries()[1].is_undone());
    assert!(restored.history_entries()[2].is_undone());
    assert_eq!(restored.operation_redo_depth(), 1);
}
