use scratchpad::app::domain::BufferState;
use std::path::PathBuf;

#[test]
fn new_buffer_starts_clean() {
    let buffer = BufferState::new("Untitled".to_owned(), "hello".to_owned(), None);

    assert_eq!(buffer.name, "Untitled");
    assert_eq!(buffer.content, "hello");
    assert_eq!(buffer.path, None);
    assert!(!buffer.is_dirty);
    assert!(buffer.temp_id.starts_with("buffer-"));
}

#[test]
fn display_name_prefixes_dirty_marker() {
    let mut buffer = BufferState::new(
        "notes.txt".to_owned(),
        String::new(),
        Some(PathBuf::from("notes.txt")),
    );

    assert_eq!(buffer.display_name(), "notes.txt");

    buffer.is_dirty = true;

    assert_eq!(buffer.display_name(), "*notes.txt");
}

#[test]
fn restored_buffer_preserves_session_metadata() {
    let buffer = BufferState::restored(
        "draft.md".to_owned(),
        "content".to_owned(),
        Some(PathBuf::from("draft.md")),
        true,
        "buffer-restore-1".to_owned(),
        "UTF-8".to_owned(),
        false,
    );

    assert!(buffer.is_dirty);
    assert_eq!(buffer.temp_id, "buffer-restore-1");
}

#[test]
fn overflow_context_uses_path_when_available() {
    let buffer = BufferState::new(
        "notes.txt".to_owned(),
        String::new(),
        Some(PathBuf::from("docs\\notes.txt")),
    );

    assert!(
        buffer
            .overflow_context_label()
            .unwrap()
            .contains("notes.txt")
    );
}
