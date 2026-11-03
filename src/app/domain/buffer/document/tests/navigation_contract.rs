//! Behavior retained from the pre-Turso editor, independent of History rendering.
use super::{TextDocument, helpers::insert_isolated_edit};

#[test]
fn navigation_moves_the_document_without_recording_edits() {
    let mut document = TextDocument::new(String::new());
    for (offset, text) in ["a", "β", "尾"].into_iter().enumerate() {
        insert_isolated_edit(&mut document, offset, text);
    }
    let ids: Vec<_> = document.history_entries().iter().map(|e| e.id).collect();
    for (expected, depth) in [("aβ", 2), ("a", 1), ("", 0)] {
        assert!(document.undo_last_operation().is_some());
        assert_eq!(document.extract_text(), expected);
        assert_eq!(document.operation_undo_depth(), depth);
    }
    assert!(document.undo_last_operation().is_none());
    for (expected, depth) in [("a", 1), ("aβ", 2), ("aβ尾", 3)] {
        assert!(document.redo_last_operation().is_some());
        assert_eq!(document.extract_text(), expected);
        assert_eq!(document.operation_undo_depth(), depth);
    }
    assert!(document.redo_last_operation().is_none());
    assert_eq!(
        ids,
        document
            .history_entries()
            .iter()
            .map(|e| e.id)
            .collect::<Vec<_>>()
    );
}

#[test]
fn separate_files_have_independent_navigation_positions() {
    let mut first = TextDocument::new(String::new());
    let mut second = TextDocument::new(String::new());
    insert_isolated_edit(&mut first, 0, "first");
    insert_isolated_edit(&mut second, 0, "second");
    first.undo_last_operation().unwrap();
    assert_eq!(first.extract_text(), "");
    assert_eq!(second.extract_text(), "second");
    assert_eq!(second.operation_redo_depth(), 0);
    first.redo_last_operation().unwrap();
    assert_eq!(first.extract_text(), "first");
}
