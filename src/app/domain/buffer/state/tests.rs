use super::BufferState;
use crate::app::domain::{CharCursor, CursorRange};

#[test]
fn unreadable_reload_preserves_dirty_metadata_and_history() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("source");
    std::fs::write(&path, "aλ\r\n".repeat(128)).unwrap();
    let (document, _, _) = super::TextDocument::from_utf8_file(&path, 0, 0).unwrap();
    let mut buffer = BufferState::new("dirty".into(), String::new(), None);
    buffer.document = document;
    buffer.is_dirty = true;
    let before = buffer.document.history_token();
    let metrics = buffer.document.piece_tree().metrics();
    buffer
        .document
        .piece_tree()
        .truncate_backing_for_test()
        .unwrap();
    let loaded = BufferState::new("loaded".into(), "replacement".into(), Some(path));
    assert!(!buffer.replace_from_loaded_buffer(loaded));
    assert!(buffer.is_dirty);
    assert_eq!(buffer.name, "dirty");
    assert!(buffer.path.is_none());
    assert_eq!(buffer.document.history_token(), before);
    assert_eq!(buffer.document.piece_tree().metrics(), metrics);
}

#[test]
fn visible_control_availability_includes_line_breaks_tabs_and_format_controls() {
    assert!(
        BufferState::new("sample.txt".to_owned(), "a\nb".to_owned(), None)
            .has_visible_control_substitutions()
    );
    assert!(
        BufferState::new("sample.txt".to_owned(), "a\tb".to_owned(), None)
            .has_visible_control_substitutions()
    );
    assert!(
        BufferState::new("sample.txt".to_owned(), "a\u{200E}b".to_owned(), None)
            .has_visible_control_substitutions()
    );
    assert!(
        BufferState::new(
            "sample.txt".to_owned(),
            "a\u{200B}\u{2060}\u{FEFF}b".to_owned(),
            None
        )
        .has_visible_control_substitutions()
    );
    assert!(
        !BufferState::new("sample.txt".to_owned(), "plain".to_owned(), None)
            .has_visible_control_substitutions()
    );
}

#[test]
fn visible_control_mode_auto_clears_when_last_substitution_disappears() {
    let mut buffer = BufferState::new("sample.txt".to_owned(), "a\u{200E}b".to_owned(), None);
    buffer.show_control_chars = true;

    buffer.replace_text("plain".to_owned());

    assert!(!buffer.show_control_chars);
}

#[test]
fn reliable_artifact_evidence_decrements_without_full_refresh() {
    let mut buffer = BufferState::new("sample.txt".to_owned(), "\u{1B}plain".to_owned(), None);

    buffer
        .replace_char_ranges_with_undo(&[(0..1, String::new())], cursor(0), cursor(0))
        .unwrap();

    assert_eq!(buffer.text(), "plain");
    assert!(!buffer.artifact_summary.has_ansi_sequences);
    assert!(!buffer.text_metadata_refresh_needed());
}

#[test]
fn unreliable_artifact_evidence_keeps_line_endings_and_defers_rescan() {
    let mut buffer = BufferState::new("sample.txt".to_owned(), "\u{1B}one\ntwo".to_owned(), None);
    buffer.artifact_summary.ansi_sequence_count = Some(0);

    buffer
        .replace_char_ranges_with_undo(&[(0..1, String::new())], cursor(0), cursor(0))
        .unwrap();

    assert_eq!(buffer.text(), "one\ntwo");
    assert_eq!(buffer.line_count, 2);
    assert_eq!(buffer.format.line_ending_counts.lf, 1);
    assert!(buffer.text_metadata_refresh_needed());
}

fn cursor(index: usize) -> CursorRange {
    CursorRange::one(CharCursor::new(index))
}
