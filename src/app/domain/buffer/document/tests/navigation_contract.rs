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

#[test]
fn initial_contents_and_alternative_paths_survive_recovery() {
    let mut document = TextDocument::new("initial\r\nλ".into());
    let initial = document.history_now();
    document.undo_last_operation().unwrap();
    assert_eq!(document.extract_text(), "");
    document.redo_last_operation().unwrap();
    insert_isolated_edit(&mut document, 0, "old ");
    let old = document.history_now();
    document.navigate_history(initial).unwrap();
    insert_isolated_edit(&mut document, 0, "new ");
    let new = document.history_now();
    document.navigate_history(initial).unwrap();
    let history = document.exported_history();
    let mut restored = TextDocument::new(document.extract_text());
    restored.restore_exported_history(history);
    assert_eq!(restored.history_tip(), new);
    restored.redo_last_operation().unwrap();
    assert_eq!(restored.extract_text(), "new initial\r\nλ");
    restored.navigate_history(old).unwrap();
    assert_eq!(restored.extract_text(), "old initial\r\nλ");
    restored.navigate_history(new).unwrap();
    assert_eq!(restored.extract_text(), "new initial\r\nλ");
    assert_eq!(restored.history_entries().len(), 3);
}

#[test]
fn reload_is_an_edit_and_forget_history_keeps_an_initial_insertion() {
    let mut document = TextDocument::new("original".into());
    insert_isolated_edit(&mut document, 0, "edit ");
    document.replace_exact_with_history("external\r\ntext");
    document.undo_last_operation().unwrap();
    assert_eq!(document.extract_text(), "edit original");
    document.redo_last_operation().unwrap();
    assert_eq!(document.extract_text(), "external\r\ntext");
    document.clear_operation_history();
    assert_eq!(document.history_entries().len(), 1);
    document.undo_last_operation().unwrap();
    assert_eq!(document.extract_text(), "");
}
