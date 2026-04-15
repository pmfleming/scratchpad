#![forbid(unsafe_code)]

use eframe::egui::{TextBuffer, text::CCursorRange};
use eframe::epaint::text::cursor::CCursor;
use scratchpad::app::domain::{
    BufferFreshness, BufferState, EncodingSource, LineEndingStyle, RestoredBufferState,
    TextDocument, TextFormatMetadata, platform_default_line_ending,
};
use std::path::PathBuf;

fn format_for(content: &str, encoding: &str, has_bom: bool) -> TextFormatMetadata {
    TextFormatMetadata::detected(
        content,
        encoding.to_owned(),
        has_bom,
        EncodingSource::ExplicitUserChoice,
        false,
    )
}

#[test]
fn new_buffer_starts_clean() {
    let buffer = BufferState::new("Untitled".to_owned(), "hello".to_owned(), None);

    assert_eq!(buffer.name, "Untitled");
    assert_eq!(buffer.text(), "hello");
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
        format: format_for("content", "UTF-8", false),
        disk_state: None,
        freshness: BufferFreshness::InSync,
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
    assert_eq!(buffer.format.line_endings, LineEndingStyle::Crlf);
}

#[test]
fn cr_only_line_endings_are_tracked_as_format_not_artifact() {
    let buffer = BufferState::new(
        "classic-mac.txt".to_owned(),
        "alpha\rbeta\r".to_owned(),
        None,
    );

    assert_eq!(buffer.line_count, 3);
    assert_eq!(buffer.format.line_endings, LineEndingStyle::Cr);
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

#[test]
fn text_document_supports_selection_cut_and_paste_operations() {
    let mut document = TextDocument::new("hello world".to_owned());
    let selection = CCursorRange::two(CCursor::new(6), CCursor::new(11));

    assert_eq!(selection.slice_str(document.as_str()), "world");

    let mut cursor = document.delete_selected(&selection);
    assert_eq!(document.as_str(), "hello ");
    assert_eq!(cursor.index, 6);

    document.insert_text_at(&mut cursor, "there", usize::MAX);
    assert_eq!(document.as_str(), "hello there");
    assert_eq!(cursor.index, 11);
}

#[test]
fn text_document_replace_with_handles_unicode_content() {
    let mut document = TextDocument::new("a🙂b".to_owned());

    TextBuffer::replace_with(&mut document, "x🌍y");

    assert_eq!(document.as_str(), "x🌍y");
}

#[test]
fn text_document_normalizes_windows_enter_key_input() {
    let mut document = TextDocument::new("alpha".to_owned());
    let expected_newline = platform_default_line_ending().as_str();
    let expected_width = expected_newline.len();

    assert_eq!(
        TextBuffer::insert_text(&mut document, "\r", 5),
        expected_width
    );
    assert_eq!(document.as_str(), format!("alpha{expected_newline}"));

    let mut document = TextDocument::new("alpha".to_owned());

    assert_eq!(
        TextBuffer::insert_text(&mut document, "\r\n", 5),
        expected_width
    );
    assert_eq!(document.as_str(), format!("alpha{expected_newline}"));
}

#[test]
fn text_document_preserves_non_newline_control_char_inserts() {
    let mut document = TextDocument::new("alpha".to_owned());

    assert_eq!(TextBuffer::insert_text(&mut document, "\rprogress", 5), 9);
    assert_eq!(document.as_str(), "alpha\rprogress");
}

#[test]
fn text_document_normalizes_multiline_paste_to_preferred_line_endings() {
    let mut document =
        TextDocument::with_preferred_line_ending("header\n".to_owned(), LineEndingStyle::Lf);

    let inserted = TextBuffer::insert_text(&mut document, "one\r\ntwo\nthree", 7);

    assert_eq!(inserted, "one\ntwo\nthree".chars().count());
    assert_eq!(document.as_str(), "header\none\ntwo\nthree");
}

#[test]
fn detected_preferred_line_ending_is_retained_after_text_changes() {
    let mut format = TextFormatMetadata::detected(
        "alpha\r\nbeta\r\n",
        "UTF-8".to_owned(),
        false,
        EncodingSource::Heuristic,
        false,
    );

    assert_eq!(format.line_endings, LineEndingStyle::Crlf);
    assert_eq!(format.preferred_line_ending_style(), LineEndingStyle::Crlf);

    format.refresh_from_text("alpha\nbeta\r\ngamma");

    assert_eq!(format.line_endings, LineEndingStyle::Mixed);
    assert_eq!(format.preferred_line_ending_style(), LineEndingStyle::Crlf);
}
