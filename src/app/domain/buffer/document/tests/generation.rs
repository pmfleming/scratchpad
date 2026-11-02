use super::TextDocument;
use super::helpers::insert_edit;

#[test]
fn history_token_invalidates_replay_clear_prune_restore_and_replacement() {
    let mut document = TextDocument::new("base".into());
    let original = document.history_token();
    insert_edit(&mut document, 0, "a");
    let edited = document.history_token();
    assert_ne!(edited, original);
    document.undo_last_operation().unwrap();
    let undone = document.history_token();
    assert_ne!(undone, edited);
    document.redo_last_operation().unwrap();
    assert_ne!(document.history_token(), edited);
    let before = document.history_token();
    let history = document.exported_history();
    document.restore_exported_history(history);
    assert_ne!(document.history_token(), before);
    let before = document.history_token();
    document.drop_oldest_history_entry();
    assert_ne!(document.history_token(), before);
    let before = document.history_token();
    document.clear_operation_history();
    assert_ne!(document.history_token(), before);
    assert_ne!(document.clone().history_token(), document.history_token());
    let before = document.history_token();
    document.replace_text(document.extract_text());
    assert_ne!(document.history_token(), before);
}

#[test]
fn generation_before_is_captured_before_direct_edit_mutation() {
    let mut document = TextDocument::new(String::new());
    let before = document.visible_generation();

    insert_edit(&mut document, 0, "a");

    let entry = &document.history_entries()[0];
    assert_eq!(entry.visible_generation_before, before);
    assert_eq!(
        entry.visible_generation_after,
        document.visible_generation()
    );
    assert!(entry.visible_generation_after > entry.visible_generation_before);
}
