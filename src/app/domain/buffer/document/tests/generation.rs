use super::TextDocument;
use super::helpers::insert_edit;

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
