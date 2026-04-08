#![forbid(unsafe_code)]

use scratchpad::app::domain::{BufferState, RestoredBufferState};
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
    let buffer = BufferState::restored(RestoredBufferState {
        id: 7,
        name: "draft.md".to_owned(),
        content: "content".to_owned(),
        path: Some(PathBuf::from("draft.md")),
        is_dirty: true,
        temp_id: "buffer-restore-1".to_owned(),
        encoding: "UTF-8".to_owned(),
        has_bom: false,
    });

    assert!(buffer.is_dirty);
    assert_eq!(buffer.temp_id, "buffer-restore-1");
}

#[test]
fn trailing_newline_counts_as_final_empty_line() {
    let buffer = BufferState::new("tail.txt".to_owned(), "alpha\n".to_owned(), None);

    assert_eq!(buffer.line_count, 2);
}

#[test]
fn normal_crlf_line_endings_do_not_mark_control_char_artifacts() {
    let buffer = BufferState::new(
        "windows.txt".to_owned(),
        "alpha\r\nbeta\r\n".to_owned(),
        None,
    );

    assert_eq!(buffer.line_count, 3);
    assert!(!buffer.artifact_summary.has_control_chars());
}

#[test]
fn ansi_escape_sequences_are_detected_as_artifacts() {
    let buffer = BufferState::new(
        "ansi.txt".to_owned(),
        "\u{1b}[31mred\u{1b}[0m".to_owned(),
        None,
    );

    assert!(buffer.artifact_summary.has_ansi_sequences);
    assert!(buffer.artifact_summary.has_control_chars());
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
