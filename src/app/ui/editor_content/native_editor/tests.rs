use super::{
    CharCursor, CursorRange, cut_selected_text, publish_active_selection,
    request_cursor_reveal_after_input, selected_text, should_rebuild_galley_after_input,
    sync_ime_output_focus,
};
use crate::app::domain::{BufferState, CursorRevealMode, EditorViewState};

#[test]
fn cursor_only_movement_inside_existing_slice_keeps_galley() {
    let before = Some(CursorRange::one(CharCursor::new(5)));
    let after = Some(CursorRange::one(CharCursor::new(50)));

    assert!(!should_rebuild_galley_after_input(
        false, None, None, before, after, 0, 100
    ));
}

#[test]
fn cursor_only_movement_outside_existing_slice_rebuilds_for_reveal() {
    let before = Some(CursorRange::one(CharCursor::new(5)));
    let after = Some(CursorRange::one(CharCursor::new(500)));

    assert!(should_rebuild_galley_after_input(
        false, None, None, before, after, 0, 100
    ));
}

#[test]
fn unchanged_input_keeps_existing_galley() {
    let cursor = Some(CursorRange::one(CharCursor::new(5)));

    assert!(!should_rebuild_galley_after_input(
        false, None, None, cursor, cursor, 0, 100
    ));
}

#[test]
fn publishing_active_selection_reports_shared_selection_changes() {
    let mut buffer = BufferState::new("sample.txt".to_owned(), "hello world".to_owned(), None);
    let mut view = EditorViewState::new(buffer.id);
    view.cursor_range = Some(CursorRange::two(0, 5));

    assert!(publish_active_selection(&mut buffer, &view, true));
    assert_eq!(buffer.active_selection, Some(0..5));
    assert!(!publish_active_selection(&mut buffer, &view, true));
}

#[test]
fn selection_copy_returns_document_controls_not_display_glyphs() {
    let text = "a\u{200E}\n\tb";
    let buffer = BufferState::new("sample.txt".to_owned(), text.to_owned(), None);
    let selection = CursorRange::two(0, buffer.current_file_length().chars);

    let copied = selected_text(&buffer, selection).unwrap();

    assert_eq!(copied, text);
    assert!(!copied.contains('\u{F003}'));
    assert!(!copied.contains('\u{240A}'));
    assert!(!copied.contains('\u{2409}'));
}

#[test]
fn cut_returns_document_controls_not_display_glyphs() {
    let mut buffer = BufferState::new("sample.txt".to_owned(), "a\u{200E}\n\tb".to_owned(), None);
    let selection = CursorRange::two(1, 4);

    let (_, cut) = cut_selected_text(&mut buffer, selection).unwrap();

    assert_eq!(cut, "\u{200E}\n\t");
    assert_eq!(buffer.text(), "ab");
}

#[test]
fn undo_after_control_char_edit_restores_document_text() {
    let mut buffer = BufferState::new("sample.txt".to_owned(), "ab".to_owned(), None);
    let previous = CursorRange::one(CharCursor::new(1));
    let next = CursorRange::one(CharCursor::new(2));

    buffer
        .replace_char_ranges_with_undo(&[(1..1, "\u{200E}".to_owned())], previous, next)
        .unwrap();
    assert_eq!(buffer.text(), "a\u{200E}b");

    let undo_selection = buffer.undo_last_text_operation().unwrap();

    assert_eq!(buffer.text(), "ab");
    assert_eq!(undo_selection, previous);
}

#[test]
fn same_line_edit_requests_horizontal_cursor_reveal() {
    let buffer = BufferState::new("sample.txt".to_owned(), "alpha beta".to_owned(), None);
    let mut view = EditorViewState::new(buffer.id);
    let previous = Some(CursorRange::one(CharCursor::new(1)));
    view.cursor_range = Some(CursorRange::one(CharCursor::new(2)));

    request_cursor_reveal_after_input(&buffer, &mut view, previous, Some(0), false, true, false);

    assert_eq!(
        view.cursor_reveal_mode(),
        Some(CursorRevealMode::KeepHorizontalVisible)
    );
}

#[test]
fn wrapped_same_line_edit_requests_vertical_cursor_reveal() {
    let buffer = BufferState::new("sample.txt".to_owned(), "alpha beta".to_owned(), None);
    let mut view = EditorViewState::new(buffer.id);
    let previous = Some(CursorRange::one(CharCursor::new(1)));
    view.cursor_range = Some(CursorRange::one(CharCursor::new(2)));

    request_cursor_reveal_after_input(&buffer, &mut view, previous, Some(0), true, true, false);

    assert_eq!(
        view.cursor_reveal_mode(),
        Some(CursorRevealMode::KeepVisible)
    );
}

#[test]
fn newline_edit_requests_vertical_cursor_reveal() {
    let buffer = BufferState::new("sample.txt".to_owned(), "alpha\nbeta".to_owned(), None);
    let mut view = EditorViewState::new(buffer.id);
    let previous = Some(CursorRange::one(CharCursor::new(4)));
    view.cursor_range = Some(CursorRange::one(CharCursor::new(7)));

    request_cursor_reveal_after_input(&buffer, &mut view, previous, Some(0), false, true, false);

    assert_eq!(
        view.cursor_reveal_mode(),
        Some(CursorRevealMode::KeepVisible)
    );
}

#[test]
fn cursor_movement_without_edit_requests_keep_visible_reveal() {
    let buffer = BufferState::new("sample.txt".to_owned(), "alpha\nbeta".to_owned(), None);
    let mut view = EditorViewState::new(buffer.id);
    let previous = Some(CursorRange::one(CharCursor::new(1)));
    view.cursor_range = Some(CursorRange::one(CharCursor::new(3)));

    request_cursor_reveal_after_input(&buffer, &mut view, previous, Some(0), false, false, false);

    assert_eq!(
        view.cursor_reveal_mode(),
        Some(CursorRevealMode::KeepVisible)
    );
}

#[test]
fn unchanged_cursor_does_not_request_reveal() {
    let buffer = BufferState::new("sample.txt".to_owned(), "alpha".to_owned(), None);
    let cursor = Some(CursorRange::one(CharCursor::new(1)));
    let mut view = EditorViewState::new(buffer.id);
    view.cursor_range = cursor;

    request_cursor_reveal_after_input(&buffer, &mut view, cursor, Some(0), false, true, false);

    assert_eq!(view.cursor_reveal_mode(), None);
}

#[test]
fn selection_drag_suppresses_cursor_reveal() {
    let buffer = BufferState::new("sample.txt".to_owned(), "alpha".to_owned(), None);
    let mut view = EditorViewState::new(buffer.id);
    view.request_cursor_reveal(CursorRevealMode::Center);
    let previous = Some(CursorRange::one(CharCursor::new(1)));
    view.cursor_range = Some(CursorRange::one(CharCursor::new(2)));

    request_cursor_reveal_after_input(&buffer, &mut view, previous, Some(0), false, true, true);

    assert_eq!(view.cursor_reveal_mode(), None);
}

#[test]
fn losing_focus_clears_ime_state() {
    let mut view = EditorViewState::new(1);
    view.ime_preedit = Some("kana".to_owned());
    assert!(view.mark_ime_output(eframe::egui::Rect::EVERYTHING, eframe::egui::Rect::ZERO));

    sync_ime_output_focus(&mut view, false);

    assert_eq!(view.ime_preedit, None);
    assert!(view.mark_ime_output(eframe::egui::Rect::EVERYTHING, eframe::egui::Rect::ZERO));
}
